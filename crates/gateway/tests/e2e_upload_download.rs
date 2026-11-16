//! End-to-end gateway test: HTTP upload -> coordinator placement -> a real
//! storage node -> HTTP download, with no PostgreSQL.
//!
//! The coordinator is a small in-memory `ClusterService` fake. It issues upload
//! and file IDs (like the real coordinator), hands out the storage node as the
//! sole replica, and records the manifest from `CompleteUpload`. This exercises
//! the full gateway path, including chunk splitting, gRPC streaming to the
//! storage node, SHA-256 verification, and ordered reconstruction.

use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use futures_util::stream::Stream;
use tokio::sync::Mutex;
use tonic::{Request as GrpcRequest, Response as GrpcResponse, Status, Streaming};
use tower::ServiceExt;
use uuid::Uuid;

use proto::storage::cluster_service_server::{ClusterService, ClusterServiceServer};
use proto::storage::{
    ChunkLocation, ChunkManifest, CompleteUploadRequest, CompleteUploadResponse,
    CreateUploadSessionRequest, CreateUploadSessionResponse, DeleteFileRequest, DeleteFileResponse,
    GetChunkPlanRequest, GetChunkPlanResponse, GetManifestRequest, GetManifestResponse,
    GetStatsRequest, GetStatsResponse, HeartbeatRequest, HeartbeatResponse, ListNodesRequest,
    ListNodesResponse, NodeInfo, ReplicateChunkRequest, ReplicateChunkResponse,
};

type BoxStream<T> = Pin<Box<dyn Stream<Item = Result<T, Status>> + Send>>;

// Deliberately different from the gateway's old hardcoded values (4 MB / 3) so
// the test proves the gateway honors the coordinator's settings.
const FAKE_CHUNK_SIZE: i64 = 2 * 1024 * 1024;
const FAKE_REPLICATION_FACTOR: i32 = 2;

#[derive(Clone)]
struct UploadRecord {
    file_id: Uuid,
    filename: String,
    total_size: i64,
    chunk_size: i64,
    replication_factor: i32,
}

#[derive(Clone)]
struct FileRecord {
    upload: UploadRecord,
    chunks: Vec<ChunkLocation>,
}

#[derive(Default)]
struct FakeState {
    node_id: Option<Uuid>,
    grpc_addr: String,
    uploads: HashMap<Uuid, UploadRecord>,
    files: HashMap<Uuid, FileRecord>,
}

#[derive(Clone)]
struct FakeCoordinator {
    state: Arc<Mutex<FakeState>>,
}

impl FakeCoordinator {
    fn new(node_id: Uuid, grpc_addr: String) -> Self {
        let state = FakeState {
            node_id: Some(node_id),
            grpc_addr,
            ..FakeState::default()
        };
        Self {
            state: Arc::new(Mutex::new(state)),
        }
    }
}

#[tonic::async_trait]
impl ClusterService for FakeCoordinator {
    type NodeHeartbeatStream = BoxStream<HeartbeatResponse>;

    async fn assign_replica(
        &self,
        _request: GrpcRequest<Streaming<ReplicateChunkRequest>>,
    ) -> Result<GrpcResponse<ReplicateChunkResponse>, Status> {
        Err(Status::unimplemented("assign_replica"))
    }

    async fn create_upload_session(
        &self,
        request: GrpcRequest<CreateUploadSessionRequest>,
    ) -> Result<GrpcResponse<CreateUploadSessionResponse>, Status> {
        let req = request.into_inner();
        let upload_id = Uuid::new_v4();
        let file_id = Uuid::new_v4();
        self.state.lock().await.uploads.insert(
            upload_id,
            UploadRecord {
                file_id,
                filename: req.filename,
                total_size: req.total_size,
                chunk_size: FAKE_CHUNK_SIZE,
                replication_factor: FAKE_REPLICATION_FACTOR,
            },
        );
        Ok(GrpcResponse::new(CreateUploadSessionResponse {
            upload_id: upload_id.to_string(),
            file_id: file_id.to_string(),
            chunk_size: FAKE_CHUNK_SIZE,
            replication_factor: FAKE_REPLICATION_FACTOR,
        }))
    }

    async fn get_chunk_plan(
        &self,
        request: GrpcRequest<GetChunkPlanRequest>,
    ) -> Result<GrpcResponse<GetChunkPlanResponse>, Status> {
        let req = request.into_inner();
        let upload_id =
            Uuid::parse_str(&req.upload_id).map_err(|e| Status::invalid_argument(e.to_string()))?;
        let state = self.state.lock().await;
        if !state.uploads.contains_key(&upload_id) {
            return Err(Status::not_found("upload session not found"));
        }
        let node_id = state.node_id.expect("node id configured");
        Ok(GrpcResponse::new(GetChunkPlanResponse {
            chunk_id: Uuid::new_v4().to_string(),
            node_ids: vec![node_id.to_string()],
        }))
    }

    async fn complete_upload(
        &self,
        request: GrpcRequest<CompleteUploadRequest>,
    ) -> Result<GrpcResponse<CompleteUploadResponse>, Status> {
        let req = request.into_inner();
        let upload_id =
            Uuid::parse_str(&req.upload_id).map_err(|e| Status::invalid_argument(e.to_string()))?;
        let file_id =
            Uuid::parse_str(&req.file_id).map_err(|e| Status::invalid_argument(e.to_string()))?;

        let mut state = self.state.lock().await;
        let upload = state
            .uploads
            .remove(&upload_id)
            .ok_or_else(|| Status::not_found("upload session not found"))?;
        // The real coordinator creates the file row under the ID it returned
        // from CreateUploadSession; a mismatched ID would violate the
        // chunks.file_id foreign key.
        if upload.file_id != file_id {
            return Err(Status::invalid_argument(
                "complete_upload file_id does not match the upload session",
            ));
        }
        let node_id = state.node_id.expect("node id configured").to_string();
        let chunks: Vec<ChunkLocation> = req
            .chunks
            .into_iter()
            .map(|c: ChunkManifest| ChunkLocation {
                chunk_id: c.chunk_id,
                chunk_index: c.chunk_index,
                size_bytes: c.size_bytes,
                sha256_hex: c.sha256_hex,
                node_ids: vec![node_id.clone()],
            })
            .collect();
        state.files.insert(file_id, FileRecord { upload, chunks });
        Ok(GrpcResponse::new(CompleteUploadResponse {
            file_id: file_id.to_string(),
        }))
    }

    async fn get_manifest(
        &self,
        request: GrpcRequest<GetManifestRequest>,
    ) -> Result<GrpcResponse<GetManifestResponse>, Status> {
        let req = request.into_inner();
        let file_id =
            Uuid::parse_str(&req.file_id).map_err(|e| Status::invalid_argument(e.to_string()))?;
        let state = self.state.lock().await;
        let file = state
            .files
            .get(&file_id)
            .ok_or_else(|| Status::not_found("file not found"))?;
        Ok(GrpcResponse::new(GetManifestResponse {
            file_id: file_id.to_string(),
            filename: file.upload.filename.clone(),
            total_size: file.upload.total_size,
            chunk_size: file.upload.chunk_size,
            chunk_count: file.chunks.len() as i32,
            replication_factor: file.upload.replication_factor,
            chunks: file.chunks.clone(),
        }))
    }

    async fn delete_file(
        &self,
        request: GrpcRequest<DeleteFileRequest>,
    ) -> Result<GrpcResponse<DeleteFileResponse>, Status> {
        let req = request.into_inner();
        let file_id =
            Uuid::parse_str(&req.file_id).map_err(|e| Status::invalid_argument(e.to_string()))?;
        let mut state = self.state.lock().await;
        let file = state
            .files
            .remove(&file_id)
            .ok_or_else(|| Status::not_found("file not found"))?;
        Ok(GrpcResponse::new(DeleteFileResponse {
            deleted: true,
            chunks: file.chunks,
        }))
    }

    async fn node_heartbeat(
        &self,
        _request: GrpcRequest<Streaming<HeartbeatRequest>>,
    ) -> Result<GrpcResponse<Self::NodeHeartbeatStream>, Status> {
        Err(Status::unimplemented("node_heartbeat"))
    }

    async fn list_nodes(
        &self,
        _request: GrpcRequest<ListNodesRequest>,
    ) -> Result<GrpcResponse<ListNodesResponse>, Status> {
        let state = self.state.lock().await;
        let node_id = state.node_id.expect("node id configured");
        Ok(GrpcResponse::new(ListNodesResponse {
            nodes: vec![NodeInfo {
                node_id: node_id.to_string(),
                addr: state.grpc_addr.clone(),
                grpc_addr: state.grpc_addr.clone(),
                healthy: true,
                available_bytes: 1 << 30,
                capacity_bytes: 1 << 30,
            }],
        }))
    }

    async fn get_stats(
        &self,
        _request: GrpcRequest<GetStatsRequest>,
    ) -> Result<GrpcResponse<GetStatsResponse>, Status> {
        let state = self.state.lock().await;
        Ok(GrpcResponse::new(GetStatsResponse {
            total_nodes: 1,
            healthy_nodes: 1,
            total_capacity_bytes: 1 << 30,
            total_available_bytes: 1 << 30,
            total_files: state.files.len() as i64,
            total_chunks: state.files.values().map(|f| f.chunks.len() as i64).sum(),
            under_replicated_chunks: 0,
        }))
    }
}

#[tokio::test]
async fn upload_then_download_roundtrip() {
    // Real storage node on an ephemeral port.
    let dir = tempfile::tempdir().expect("tempdir");
    let node_listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind node");
    let node_addr = format!("http://{}", node_listener.local_addr().expect("node addr"));
    let node_id = Uuid::new_v4();
    let node = Arc::new(
        storage_node::StorageNode::new(node_addr.clone(), node_addr.clone(), dir.path())
            .await
            .expect("create node"),
    );
    let (node_shutdown_tx, node_shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    let node_task = tokio::spawn(async move {
        let _ =
            storage_node::StorageNodeServer::serve_with_listener(node_listener, node, async move {
                let _ = node_shutdown_rx.await;
            })
            .await;
    });

    // In-memory coordinator on an ephemeral port.
    let coord_listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind coordinator");
    let coord_addr = format!(
        "http://{}",
        coord_listener.local_addr().expect("coord addr")
    );
    let fake = FakeCoordinator::new(node_id, node_addr.clone());
    let coord_task = tokio::spawn(async move {
        tonic::transport::Server::builder()
            .add_service(ClusterServiceServer::new(fake))
            .serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(
                coord_listener,
            ))
            .await
            .expect("coordinator serve");
    });

    let app = gateway::router(Arc::new(gateway::AppState::new(&coord_addr)));

    // More than one 4 MB chunk so multi-chunk ordering is exercised.
    let payload: Vec<u8> = (0..(5 * 1024 * 1024 + 123))
        .map(|i| (i % 251) as u8)
        .collect();

    let upload_req = Request::builder()
        .method("POST")
        .uri("/api/v1/files")
        .header("x-filename", "e2e.bin")
        .header("content-length", payload.len().to_string())
        .body(Body::from(payload.clone()))
        .expect("upload request");
    let upload_resp = app.clone().oneshot(upload_req).await.expect("upload");
    let upload_status = upload_resp.status();
    let upload_body = axum::body::to_bytes(upload_resp.into_body(), usize::MAX)
        .await
        .expect("upload body");
    assert_eq!(
        upload_status,
        StatusCode::OK,
        "upload failed: {}",
        String::from_utf8_lossy(&upload_body)
    );

    let json: serde_json::Value =
        serde_json::from_slice(&upload_body).expect("upload response json");
    assert_eq!(
        json["chunk_count"].as_u64(),
        Some(3),
        "expected three chunks"
    );
    assert_eq!(json["replication_factor"].as_i64(), Some(2));
    assert_eq!(json["chunk_size"].as_i64(), Some(FAKE_CHUNK_SIZE));
    let file_id = json["file_id"].as_str().expect("file_id").to_string();

    let download_req = Request::builder()
        .uri(format!("/api/v1/files/{file_id}/download"))
        .body(Body::empty())
        .expect("download request");
    let download_resp = app.clone().oneshot(download_req).await.expect("download");
    assert_eq!(download_resp.status(), StatusCode::OK);
    let expected_len = payload.len().to_string();
    assert_eq!(
        download_resp
            .headers()
            .get("content-length")
            .and_then(|v| v.to_str().ok()),
        Some(expected_len.as_str())
    );
    let downloaded = axum::body::to_bytes(download_resp.into_body(), usize::MAX)
        .await
        .expect("download body");
    assert_eq!(
        downloaded.as_ref(),
        payload.as_slice(),
        "downloaded bytes must match the uploaded payload"
    );

    // Three chunks should exist on disk before deletion.
    assert_eq!(
        std::fs::read_dir(dir.path())
            .expect("read storage dir")
            .count(),
        3
    );

    // Delete: metadata first, then the chunk bytes on the storage node.
    let delete_req = Request::builder()
        .method("DELETE")
        .uri(format!("/api/v1/files/{file_id}"))
        .body(Body::empty())
        .expect("delete request");
    let delete_resp = app.clone().oneshot(delete_req).await.expect("delete");
    let delete_status = delete_resp.status();
    let delete_body = axum::body::to_bytes(delete_resp.into_body(), usize::MAX)
        .await
        .expect("delete body");
    assert_eq!(
        delete_status,
        StatusCode::OK,
        "delete failed: {}",
        String::from_utf8_lossy(&delete_body)
    );
    let delete_json: serde_json::Value =
        serde_json::from_slice(&delete_body).expect("delete response json");
    assert_eq!(delete_json["status"], "deleted");
    assert_eq!(delete_json["chunks"].as_u64(), Some(3));
    assert_eq!(delete_json["chunk_replicas_deleted"].as_u64(), Some(3));
    assert_eq!(delete_json["chunk_replicas_failed"].as_u64(), Some(0));
    assert_eq!(
        std::fs::read_dir(dir.path())
            .expect("read storage dir")
            .count(),
        0,
        "chunk bytes must be removed from the storage node"
    );

    // Metadata is gone: download and metadata lookup both return 404.
    let download_req = Request::builder()
        .uri(format!("/api/v1/files/{file_id}/download"))
        .body(Body::empty())
        .expect("download request");
    let resp = app.clone().oneshot(download_req).await.expect("download");
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);

    let get_req = Request::builder()
        .uri(format!("/api/v1/files/{file_id}"))
        .body(Body::empty())
        .expect("get request");
    let resp = app.oneshot(get_req).await.expect("get");
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);

    drop(node_shutdown_tx);
    let _ = node_task.await;
    coord_task.abort();
}
