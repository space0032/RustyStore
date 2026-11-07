//! Gateway HTTP application.
//!
//! Client-facing Axum application: streaming upload/download, file metadata,
//! cluster stats, and health. Talks to the Coordinator over gRPC for
//! placement and manifest lookups.

mod app;
mod chunk_utils;
mod error;
mod handlers;
mod routes;

pub use app::AppState;

use axum::Router;
use std::net::SocketAddr;
use std::sync::Arc;
use tracing::info;

pub async fn run(
    addr: SocketAddr,
    coordinator_addr: String,
) -> Result<(), Box<dyn std::error::Error>> {
    let state = Arc::new(AppState::new(&coordinator_addr));
    let app = routes::routes(Arc::clone(&state));

    info!("gateway listening on {addr}, coordinator at {coordinator_addr}");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

pub fn router(state: Arc<AppState>) -> Router {
    routes::routes(state)
}
