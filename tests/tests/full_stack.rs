//! Full-stack integration tests: real PostgreSQL, coordinator, storage nodes,
//! and the gateway HTTP router.
//!
//! These tests run only when `TEST_DATABASE_URL` (or `DATABASE_URL`) points at
//! a disposable PostgreSQL database, mirroring the metadata integration tests.
//! Provision one with `scripts/test-postgres.sh` / `.ps1`.

use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use coordinator::{CoordinatorClient, CoordinatorServer};
use futures_util::stream;
use metadata::MetadataStore;
use proto::storage::{HeartbeatRequest, RegisterRequest, node_service_client::NodeServiceClient};
use sqlx::postgres::{PgPool, PgPoolOptions};
use storage_node::{StorageNode, StorageNodeServer};
use tempfile::TempDir;
use tower::ServiceExt;
use uuid::Uuid;

const CHUNK_SIZE: i64 = 1024 * 1024;
const NODE_CAPACITY: i64 = 1024 * 1024 * 1024;

fn database_url() -> Option<String> {
    std::env::var("TEST_DATABASE_URL")
        .or_else(|_| std::env::var("DATABASE_URL"))
        .ok()
}

fn db_lock() -> &'static tokio::sync::Mutex<()> {
    static LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
}

struct TestNode {
    node_id: Uuid,
    dir: TempDir,
    shutdown: Option<tokio::sync::oneshot::Sender<()>>,
    task: Option<tokio::task::JoinHandle<()>>,
    heartbeat: tokio::task::JoinHandle<()>,
}

struct TestStack {
    coord_addr: String,
    coord_shutdown: Option<tokio::sync::oneshot::Sender<()>>,
    coord_task: Option<tokio::task::JoinHandle<()>>,
    nodes: Vec<TestNode>,
    app: Router,
    pool: PgPool,
}

impl TestStack {
    fn stored_chunk_count(&self) -> usize {
        self.nodes
            .iter()
            .map(|n| {
                std::fs::read_dir(n.dir.path())
                    .map(|entries| entries.count())
                    .unwrap_or(0)
            })
            .sum()
    }

    async fn stop_node(&mut self, node_id: Uuid) {
        let node = self
            .nodes
            .iter_mut()
            .find(|n| n.node_id == node_id)
            .expect("node exists");
        if let Some(tx) = node.shutdown.take() {
            let _ = tx.send(());
        }
        node.heartbeat.abort();
        if let Some(task) = node.task.take() {
            let _ = task.await;
        }
    }

    async fn shutdown(mut self) {
        for node in &mut self.nodes {
            if let Some(tx) = node.shutdown.take() {
                let _ = tx.send(());
            }
            node.heartbeat.abort();
            if let Some(task) = node.task.take() {
                let _ = task.await;
            }
        }
        if let Some(tx) = self.coord_shutdown.take() {
            let _ = tx.send(());
        }
        if let Some(task) = self.coord_task.take() {
            let _ = task.await;
        }
    }
}

async fn start_stack(replication_factor: i32, node_count: usize) -> Option<TestStack> {
    let url = database_url()?;

    MetadataStore::connect_and_migrate(&url)
        .await
        .expect("metadata schema");
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&url)
        .await
        .expect("connect database");
    sqlx::raw_sql(
        "TRUNCATE replication_tasks, chunk_replicas, chunks, upload_sessions, files, storage_nodes CASCADE",
    )
    .execute(&pool)
    .await
    .expect("reset database");

    let coord_listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind coordinator");
    let coord_addr = format!(
        "http://{}",
        coord_listener.local_addr().expect("coordinator addr")
    );
    let (coord_shutdown, coord_rx) = tokio::sync::oneshot::channel();
    let store = Arc::new(
        MetadataStore::connect_and_migrate(&url)
            .await
            .expect("coordinator store"),
    );
    let coord_task = tokio::spawn(async move {
        let _ = CoordinatorServer::serve_with_listener(
            coord_listener,
            store,
            replication_factor,
            CHUNK_SIZE,
            async move {
                let _ = coord_rx.await;
            },
        )
        .await;
    });

    wait_for_coordinator(&coord_addr).await;

    let mut nodes = Vec::new();
    for _ in 0..node_count {
        let dir = tempfile::tempdir().expect("tempdir");
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind node");
        let grpc_addr = format!("http://{}", listener.local_addr().expect("node addr"));
        let node = Arc::new(
            StorageNode::new(grpc_addr.clone(), grpc_addr.clone(), dir.path())
                .await
                .expect("create node"),
        );

        let (shutdown, shutdown_rx) = tokio::sync::oneshot::channel();
        let serving_node = Arc::clone(&node);
        let task = tokio::spawn(async move {
            let _ = StorageNodeServer::serve_with_listener(listener, serving_node, async move {
                let _ = shutdown_rx.await;
            })
            .await;
        });

        let mut client = NodeServiceClient::connect(coord_addr.clone())
            .await
            .expect("connect coordinator");
        let response = client
            .register(tonic::Request::new(stream::iter(vec![RegisterRequest {
                addr: grpc_addr.clone(),
                grpc_addr: grpc_addr.clone(),
                capacity_bytes: NODE_CAPACITY,
                node_version: "test".to_string(),
            }])))
            .await
            .expect("register node")
            .into_inner();
        let node_id = Uuid::parse_str(&response.node_id).expect("node id");
        node.set_node_id(node_id);

        let heartbeat = spawn_heartbeat(coord_addr.clone(), Arc::clone(&node), node_id);
        nodes.push(TestNode {
            node_id,
            dir,
            shutdown: Some(shutdown),
            task: Some(task),
            heartbeat,
        });
    }

    wait_for_healthy_nodes(&coord_addr, node_count).await;

    let app = gateway::router(Arc::new(gateway::AppState::new(&coord_addr)));

    Some(TestStack {
        coord_addr,
        coord_shutdown: Some(coord_shutdown),
        coord_task: Some(coord_task),
        nodes,
        app,
        pool,
    })
}

async fn wait_for_coordinator(addr: &str) {
    for _ in 0..50 {
        if CoordinatorClient::connect(addr).await.is_ok() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("coordinator did not become reachable");
}

async fn wait_for_healthy_nodes(addr: &str, expected: usize) {
    for _ in 0..50 {
        if let Ok(mut client) = CoordinatorClient::connect(addr).await
            && let Ok(nodes) = client.list_nodes().await
            && nodes.iter().filter(|n| n.healthy).count() >= expected
        {
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("storage nodes did not register as healthy");
}

fn spawn_heartbeat(
    coordinator_addr: String,
    node: Arc<StorageNode>,
    node_id: Uuid,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            let Ok(mut client) = NodeServiceClient::connect(coordinator_addr.clone()).await else {
                tokio::time::sleep(Duration::from_secs(1)).await;
                continue;
            };

            let node = Arc::clone(&node);
            let outbound = async_stream::stream! {
                let mut interval = tokio::time::interval(Duration::from_secs(3));
                loop {
                    interval.tick().await;
                    let (available_bytes, used_bytes) = match node.capacity().await {
                        Ok(c) => (c.available_bytes as i64, c.used_bytes as i64),
                        Err(_) => (0, 0),
                    };
                    yield HeartbeatRequest {
                        node_id: node_id.to_string(),
                        available_bytes,
                        used_bytes,
                        chunk_count: 0,
                    };
                }
            };

            if let Ok(response) = client.heartbeat(tonic::Request::new(outbound)).await {
                let mut inbound = response.into_inner();
                while let Ok(Some(_)) = inbound.message().await {}
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    })
}

async fn upload(app: &Router, filename: &str, payload: &[u8]) -> (StatusCode, serde_json::Value) {
    let request = Request::builder()
        .method("POST")
        .uri("/api/v1/files")
        .header("x-filename", filename)
        .header("content-length", payload.len().to_string())
        .body(Body::from(payload.to_vec()))
        .expect("upload request");

    let response = app.clone().oneshot(request).await.expect("upload");
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("upload body");
    let json = serde_json::from_slice(&body).unwrap_or(serde_json::Value::Null);
    (status, json)
}

async fn download(app: &Router, file_id: &str) -> (StatusCode, Vec<u8>) {
    let request = Request::builder()
        .uri(format!("/api/v1/files/{file_id}/download"))
        .body(Body::empty())
        .expect("download request");
    let response = app.clone().oneshot(request).await.expect("download");
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("download body");
    (status, body.to_vec())
}

fn payload(len: usize, seed: u8) -> Vec<u8> {
    (0..len)
        .map(|i| ((i as u64 + seed as u64) % 251) as u8)
        .collect()
}

#[tokio::test]
async fn full_stack_upload_download_list_delete() {
    let Some(_guard) = database_url().map(|_| db_lock().lock()) else {
        return;
    };
    let _guard = _guard.await;
    let Some(stack) = start_stack(2, 3).await else {
        return;
    };

    let data = payload(3 * 1024 * 1024 + 123, 7);
    let (status, json) = upload(&stack.app, "full-stack.bin", &data).await;
    assert_eq!(status, StatusCode::OK, "upload failed: {json}");
    assert_eq!(json["chunk_count"].as_u64(), Some(4));
    assert_eq!(json["replication_factor"].as_i64(), Some(2));
    assert_eq!(json["chunk_size"].as_i64(), Some(CHUNK_SIZE));
    let file_id = json["file_id"].as_str().expect("file id").to_string();

    assert_eq!(
        stack.stored_chunk_count(),
        4 * 2,
        "every chunk must be stored on both replicas"
    );

    let list = Request::builder()
        .uri("/api/v1/files?limit=10")
        .body(Body::empty())
        .expect("list request");
    let list_response = stack.app.clone().oneshot(list).await.expect("list");
    assert_eq!(list_response.status(), StatusCode::OK);
    let list_body = axum::body::to_bytes(list_response.into_body(), usize::MAX)
        .await
        .expect("list body");
    let list_json: serde_json::Value = serde_json::from_slice(&list_body).expect("list json");
    assert_eq!(list_json["files"][0]["file_id"], file_id);
    assert_eq!(list_json["files"][0]["status"], "committed");

    let (status, downloaded) = download(&stack.app, &file_id).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(downloaded, data);

    let delete = Request::builder()
        .method("DELETE")
        .uri(format!("/api/v1/files/{file_id}"))
        .body(Body::empty())
        .expect("delete request");
    let delete_response = stack.app.clone().oneshot(delete).await.expect("delete");
    assert_eq!(delete_response.status(), StatusCode::OK);
    let delete_body = axum::body::to_bytes(delete_response.into_body(), usize::MAX)
        .await
        .expect("delete body");
    let delete_json: serde_json::Value = serde_json::from_slice(&delete_body).expect("delete json");
    assert_eq!(delete_json["status"], "deleted");
    assert_eq!(delete_json["chunks"].as_u64(), Some(4));
    assert_eq!(delete_json["chunk_replicas_deleted"].as_u64(), Some(8));
    assert_eq!(delete_json["chunk_replicas_failed"].as_u64(), Some(0));
    assert_eq!(stack.stored_chunk_count(), 0);

    let (status, _) = download(&stack.app, &file_id).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    stack.shutdown().await;
}

#[tokio::test]
async fn download_survives_a_storage_node_failure() {
    let Some(_guard) = database_url().map(|_| db_lock().lock()) else {
        return;
    };
    let _guard = _guard.await;
    let mut stack = start_stack(2, 3).await.expect("stack");

    let data = payload(2 * 1024 * 1024 + 17, 11);
    let (status, json) = upload(&stack.app, "failover.bin", &data).await;
    assert_eq!(status, StatusCode::OK, "upload failed: {json}");
    let file_id = json["file_id"].as_str().expect("file id").to_string();

    let mut client = CoordinatorClient::connect(&stack.coord_addr)
        .await
        .expect("coordinator client");
    let manifest = client
        .get_manifest(Uuid::parse_str(&file_id).expect("file id"))
        .await
        .expect("manifest");
    let dead_node = manifest.chunks[0].node_ids[0];

    stack.stop_node(dead_node).await;

    let (status, downloaded) = download(&stack.app, &file_id).await;
    assert_eq!(status, StatusCode::OK, "download must use another replica");
    assert_eq!(downloaded, data);

    stack.shutdown().await;
}

#[tokio::test]
async fn concurrent_uploads_are_all_committed() {
    let Some(_guard) = database_url().map(|_| db_lock().lock()) else {
        return;
    };
    let _guard = _guard.await;
    let Some(stack) = start_stack(2, 3).await else {
        return;
    };

    let mut handles = Vec::new();
    for i in 0..4 {
        let app = stack.app.clone();
        handles.push(tokio::spawn(async move {
            let data = payload(200 * 1024 + i * 37, i as u8);
            let (status, json) = upload(&app, &format!("concurrent-{i}.bin"), &data).await;
            assert_eq!(status, StatusCode::OK, "upload {i} failed: {json}");
            json["file_id"].as_str().expect("file id").to_string()
        }));
    }

    let mut file_ids = Vec::new();
    for handle in handles {
        file_ids.push(handle.await.expect("upload task"));
    }
    file_ids.sort();
    file_ids.dedup();
    assert_eq!(file_ids.len(), 4);

    let list = Request::builder()
        .uri("/api/v1/files?limit=10")
        .body(Body::empty())
        .expect("list request");
    let list_response = stack.app.clone().oneshot(list).await.expect("list");
    let list_body = axum::body::to_bytes(list_response.into_body(), usize::MAX)
        .await
        .expect("list body");
    let list_json: serde_json::Value = serde_json::from_slice(&list_body).expect("list json");
    assert_eq!(list_json["files"].as_array().map(Vec::len), Some(4));

    stack.shutdown().await;
}

#[tokio::test]
async fn recovery_replaces_a_failed_replica() {
    let Some(_guard) = database_url().map(|_| db_lock().lock()) else {
        return;
    };
    let _guard = _guard.await;
    let mut stack = start_stack(2, 3).await.expect("stack");

    let data = payload(512 * 1024 + 31, 23);
    let (status, json) = upload(&stack.app, "recovery.bin", &data).await;
    assert_eq!(status, StatusCode::OK, "upload failed: {json}");
    let file_id = json["file_id"].as_str().expect("file id").to_string();

    let mut client = CoordinatorClient::connect(&stack.coord_addr)
        .await
        .expect("coordinator client");
    let manifest = client
        .get_manifest(Uuid::parse_str(&file_id).expect("file id"))
        .await
        .expect("manifest");
    let chunk = &manifest.chunks[0];
    let dead_node = chunk.node_ids[0];
    let spare = stack
        .nodes
        .iter()
        .map(|n| n.node_id)
        .find(|id| !chunk.node_ids.contains(id))
        .expect("spare node");

    stack.stop_node(dead_node).await;

    // Simulate lease expiry plus loss of the durable copy. The detector should
    // create a task and the scheduler should copy the chunk to the spare node.
    sqlx::query(
        "UPDATE storage_nodes SET healthy=false, lease_expiry=now() - interval '1 second' WHERE node_id=$1",
    )
    .bind(dead_node)
    .execute(&stack.pool)
    .await
    .expect("expire lease");
    sqlx::query("UPDATE chunk_replicas SET durable=false WHERE chunk_id=$1 AND node_id=$2")
        .bind(chunk.chunk_id)
        .bind(dead_node)
        .execute(&stack.pool)
        .await
        .expect("drop durable flag");

    let deadline = Instant::now() + Duration::from_secs(40);
    loop {
        let durable: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM chunk_replicas WHERE chunk_id=$1 AND durable=true",
        )
        .bind(chunk.chunk_id)
        .fetch_one(&stack.pool)
        .await
        .expect("count durable replicas");
        let on_spare: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM chunk_replicas WHERE chunk_id=$1 AND node_id=$2 AND durable=true",
        )
        .bind(chunk.chunk_id)
        .bind(spare)
        .fetch_one(&stack.pool)
        .await
        .expect("count spare replica");

        if durable.0 >= 2 && on_spare.0 == 1 {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "recovery did not add a durable replacement replica"
        );
        tokio::time::sleep(Duration::from_millis(500)).await;
    }

    let (status, downloaded) = download(&stack.app, &file_id).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(downloaded, data);

    let surviving_source = chunk
        .node_ids
        .iter()
        .copied()
        .find(|id| *id != dead_node)
        .expect("surviving source replica");
    let (task_status, source, target): (String, Option<Uuid>, Uuid) = sqlx::query_as(
        "SELECT status, source_node_id, target_node_id FROM replication_tasks \
         WHERE chunk_id=$1 AND status='done' ORDER BY updated_at DESC LIMIT 1",
    )
    .bind(chunk.chunk_id)
    .fetch_one(&stack.pool)
    .await
    .expect("completed replication task");
    assert_eq!(task_status, "done");
    assert_eq!(source, Some(surviving_source), "actual source persisted");
    assert_eq!(target, spare, "actual target persisted");

    stack.shutdown().await;
}
