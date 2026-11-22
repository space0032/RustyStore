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

    // One shutdown signal fans out to both the gRPC and HTTP servers so a
    // SIGTERM drains everything instead of leaving the probe port bound.
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    let signal_tx = shutdown_tx.clone();
    let signal_task = tokio::spawn(async move {
        common::shutdown::signal().await;
        tracing::info!("shutdown signal received; draining coordinator");
        let _ = shutdown_tx.send(true);
    });
    let http_task = tokio::spawn(coordinator::http::serve_with_shutdown(
        http_addr,
        ready,
        wait_for_shutdown(shutdown_rx.clone()),
    ));

    tracing::info!(
        "coordinator listening on {addr} (replication_factor={replication_factor}, chunk_size={chunk_size_bytes})"
    );

    let result = CoordinatorServer::run_with_shutdown(
        addr,
        store,
        replication_factor,
        chunk_size_bytes,
        wait_for_shutdown(shutdown_rx),
    )
    .await;

    // The gRPC server also stops if it fails to start; make sure the HTTP
    // surface is drained either way.
    let _ = signal_tx.send(true);
    match http_task.await {
        Ok(Ok(())) => {}
        Ok(Err(e)) => tracing::error!("coordinator HTTP server failed: {e}"),
        Err(e) if e.is_cancelled() => {}
        Err(e) => tracing::error!("coordinator HTTP task failed: {e}"),
    }
    signal_task.abort();
    result
}

/// Resolve once the shared shutdown flag flips to `true`.
async fn wait_for_shutdown(mut rx: tokio::sync::watch::Receiver<bool>) {
    if *rx.borrow() {
        return;
    }
    let _ = rx.changed().await;
}
