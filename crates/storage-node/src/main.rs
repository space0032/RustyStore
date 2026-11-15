//! Storage node service entry point.
//!
//! Registers with the coordinator, streams periodic heartbeats to renew its
//! lease, and serves the NodeService/ChunkService/ClusterService gRPC API.

use std::sync::Arc;
use std::time::Duration;

use proto::storage::node_service_client::NodeServiceClient;
use proto::storage::{HeartbeatRequest, RegisterRequest};
use storage_node::{StorageNode, StorageNodeServer};
use tonic::Request;
use uuid::Uuid;

/// Ensure an endpoint has an HTTP scheme so tonic's `Channel::from_shared` works.
fn normalize_endpoint(addr: &str) -> String {
    if addr.starts_with("http://") || addr.starts_with("https://") {
        addr.to_string()
    } else {
        format!("http://{addr}")
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    observability::init()?;
    observability::register_metrics()?;

    let coordinator_addr = normalize_endpoint(&common::config::env_string(
        "COORDINATOR_ADDR",
        "127.0.0.1:50051",
    ));
    let node_addr = common::config::env_string("NODE_ADDR", "0.0.0.0:50051");
    let advertised_addr = common::config::env_string("ADVERTISED_ADDR", &node_addr);
    let storage_path = common::config::env_string("STORAGE_PATH", "./data/chunks");
    common::config::require_non_empty("STORAGE_PATH", &storage_path)?;
    let capacity_bytes: i64 =
        common::config::parse_env("NODE_CAPACITY_BYTES", 100 * 1024 * 1024 * 1024)?;
    common::config::require_positive("NODE_CAPACITY_BYTES", capacity_bytes)?;

    let addr = common::config::parse_addr_value("NODE_ADDR", &node_addr)?;

    let node =
        Arc::new(StorageNode::new(advertised_addr.clone(), node_addr.clone(), storage_path).await?);

    // Prometheus metrics + health/readiness on a separate HTTP port. Started
    // before registration so liveness works while waiting for the coordinator.
    let http_addr = common::config::parse_addr("STORAGE_NODE_HTTP_ADDR", "0.0.0.0:8082")?;
    let ready_node = Arc::clone(&node);
    let ready: storage_node::http::ReadyCheck = Arc::new(move || {
        let node = Arc::clone(&ready_node);
        Box::pin(async move { node.node_id().is_some() && node.capacity().await.is_ok() })
    });
    tokio::spawn(async move {
        if let Err(e) = storage_node::http::serve(http_addr, ready).await {
            tracing::error!("storage node HTTP server failed: {e}");
        }
    });

    // Register with the coordinator, retrying until it is reachable.
    let node_id = register_with_retry(
        &coordinator_addr,
        &advertised_addr,
        &node_addr,
        capacity_bytes,
    )
    .await;
    node.set_node_id(node_id);
    tracing::info!("registered with coordinator as node_id={node_id}");

    // Heartbeat loop renews the lease for the process lifetime.
    {
        let node = Arc::clone(&node);
        let coordinator_addr = coordinator_addr.clone();
        tokio::spawn(async move {
            heartbeat_loop(coordinator_addr, node, node_id).await;
        });
    }

    tracing::info!("storage node listening on {addr} (advertised as {advertised_addr})");
    StorageNodeServer::run(addr, node).await
}

async fn register_with_retry(
    coordinator_addr: &str,
    advertised_addr: &str,
    grpc_addr: &str,
    capacity_bytes: i64,
) -> Uuid {
    loop {
        match register_once(coordinator_addr, advertised_addr, grpc_addr, capacity_bytes).await {
            Ok(node_id) => return node_id,
            Err(e) => {
                tracing::warn!("registration failed: {e}; retrying in 3s");
                tokio::time::sleep(Duration::from_secs(3)).await;
            }
        }
    }
}

async fn register_once(
    coordinator_addr: &str,
    advertised_addr: &str,
    grpc_addr: &str,
    capacity_bytes: i64,
) -> Result<Uuid, Box<dyn std::error::Error + Send + Sync>> {
    let mut client = NodeServiceClient::connect(coordinator_addr.to_string()).await?;

    let request = RegisterRequest {
        addr: advertised_addr.to_string(),
        grpc_addr: grpc_addr.to_string(),
        capacity_bytes,
        node_version: env!("CARGO_PKG_VERSION").to_string(),
    };

    let stream = futures_util::stream::iter(vec![request]);
    let response = client.register(Request::new(stream)).await?.into_inner();
    let node_id = Uuid::parse_str(&response.node_id)?;
    Ok(node_id)
}

/// Stream heartbeats, reconnecting if the stream breaks.
async fn heartbeat_loop(coordinator_addr: String, node: Arc<StorageNode>, node_id: Uuid) {
    loop {
        if let Err(e) = heartbeat_stream(&coordinator_addr, Arc::clone(&node), node_id).await {
            tracing::warn!("heartbeat stream error: {e}; reconnecting in 5s");
        }
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}

async fn heartbeat_stream(
    coordinator_addr: &str,
    node: Arc<StorageNode>,
    node_id: Uuid,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut client = NodeServiceClient::connect(coordinator_addr.to_string()).await?;
    let node_id_str = node_id.to_string();

    let outbound = async_stream::stream! {
        let mut interval = tokio::time::interval(Duration::from_secs(5));
        loop {
            interval.tick().await;
            let (available_bytes, used_bytes) = match node.capacity().await {
                Ok(c) => (c.available_bytes as i64, c.used_bytes as i64),
                Err(_) => (0, 0),
            };
            yield HeartbeatRequest {
                node_id: node_id_str.clone(),
                available_bytes,
                used_bytes,
                chunk_count: 0,
            };
        }
    };

    let mut inbound = client.heartbeat(Request::new(outbound)).await?.into_inner();
    while let Some(resp) = inbound.message().await? {
        tracing::debug!("heartbeat ack: healthy={}", resp.healthy);
    }
    Ok(())
}
