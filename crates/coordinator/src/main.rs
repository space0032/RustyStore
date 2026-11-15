//! Coordinator service entry point.
//!
//! Reads configuration from the environment, connects to PostgreSQL (applying
//! the idempotent schema), and serves the gRPC ClusterService/NodeService with
//! background lease, under-replication, and recovery tasks.

use std::sync::Arc;

use coordinator::CoordinatorServer;
use metadata::MetadataStore;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    observability::init()?;
    observability::register_metrics()?;

    let database_url = common::config::env_string(
        "DATABASE_URL",
        "postgres://rustystore:rustystore@localhost:5432/rustystore",
    );
    let addr = common::config::parse_addr("COORDINATOR_ADDR", "0.0.0.0:50051")?;
    let replication_factor: i32 = common::config::parse_env("REPLICATION_FACTOR", 3)?;
    common::config::require_positive("REPLICATION_FACTOR", replication_factor as i64)?;
    let chunk_size_bytes: i64 = common::config::parse_env("CHUNK_SIZE_BYTES", 4 * 1024 * 1024)?;
    common::config::require_positive("CHUNK_SIZE_BYTES", chunk_size_bytes)?;

    tracing::info!("connecting to PostgreSQL and applying schema");
    let store = Arc::new(MetadataStore::connect_and_migrate(&database_url).await?);

    // Prometheus metrics + health/readiness on a separate HTTP port.
    let http_addr = common::config::parse_addr("COORDINATOR_HTTP_ADDR", "0.0.0.0:8081")?;
    let ready_store = Arc::clone(&store);
    let ready: coordinator::http::ReadyCheck = Arc::new(move || {
        let store = Arc::clone(&ready_store);
        Box::pin(async move { store.ping().await.is_ok() })
    });
    tokio::spawn(async move {
        if let Err(e) = coordinator::http::serve(http_addr, ready).await {
            tracing::error!("coordinator HTTP server failed: {e}");
        }
    });

    tracing::info!(
        "coordinator listening on {addr} (replication_factor={replication_factor}, chunk_size={chunk_size_bytes})"
    );

    CoordinatorServer::run(addr, store, replication_factor, chunk_size_bytes).await
}
