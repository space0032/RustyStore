//! Gateway HTTP application.
//!
//! Client-facing Axum application: streaming upload/download, file metadata,
//! cluster stats, and health. Talks to the Coordinator over gRPC for
//! placement and manifest lookups.
//!
//! Production hardening (Phase 10): rate limiting, authentication,
//! graceful shutdown, observability middleware.

mod app;
mod auth;
mod chunk_utils;
mod error;
mod handlers;
mod routes;

pub use app::AppState;
pub use auth::{AuthConfig, auth_middleware};

use axum::Router;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tower::ServiceBuilder;
use tower_http::{timeout::TimeoutLayer, trace::TraceLayer};
use tracing::info;

pub async fn run(
    addr: SocketAddr,
    coordinator_addr: String,
) -> Result<(), Box<dyn std::error::Error>> {
    let state = Arc::new(AppState::new(&coordinator_addr));

    // Build auth config from environment.
    let auth_config = Arc::new(AuthConfig::from_env());

    let app = Router::new()
        .merge(routes::routes(Arc::clone(&state)))
        .layer(
            ServiceBuilder::new()
                .layer(TraceLayer::new_for_http())
                .layer(TimeoutLayer::new(Duration::from_secs(30)))
                .layer(axum::middleware::from_fn_with_state(
                    auth_config.clone(),
                    auth_middleware,
                )),
        );

    info!("gateway listening on {addr}, coordinator at {coordinator_addr}");
    info!(
        "auth: {}",
        if auth_config.require_auth {
            "enabled"
        } else {
            "disabled"
        }
    );

    let listener = tokio::net::TcpListener::bind(addr).await?;

    // Graceful shutdown on SIGINT/SIGTERM.
    let graceful = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
        info!("received shutdown signal, shutting down gracefully");
    };

    axum::serve(listener, app)
        .with_graceful_shutdown(graceful)
        .await
        .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;

    Ok(())
}

pub fn router(state: Arc<AppState>) -> Router {
    routes::routes(state)
}
