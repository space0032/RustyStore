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
mod rate_limit;
mod request_id;
mod routes;

pub use app::AppState;
pub use auth::{AuthConfig, auth_middleware};
pub use rate_limit::{RateLimitConfig, RateLimiter, rate_limit_middleware};
pub use request_id::{REQUEST_ID_HEADER, request_id_middleware, resolve_request_id};

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
    let max_upload_bytes: u64 = common::config::parse_env("MAX_UPLOAD_BYTES", 0)?;
    let state = Arc::new(AppState::new(&coordinator_addr).with_max_upload_bytes(max_upload_bytes));

    // Register Prometheus collectors (idempotent).
    observability::register_metrics()?;

    // Build auth + rate-limit configuration from the environment. Invalid
    // values fail startup instead of silently weakening protection.
    let auth_config = Arc::new(AuthConfig::try_from_env()?);
    let rate_config = RateLimitConfig::try_from_env()?;
    let rate_limiter = Arc::new(RateLimiter::new(rate_config.rps, rate_config.burst));

    let app = Router::new()
        .merge(routes::routes(Arc::clone(&state)))
        .layer(
            ServiceBuilder::new()
                .layer(axum::middleware::from_fn(request_id_middleware))
                .layer(TraceLayer::new_for_http())
                .layer(TimeoutLayer::new(Duration::from_secs(30)))
                .layer(axum::middleware::from_fn_with_state(
                    auth_config.clone(),
                    auth_middleware,
                ))
                .layer(axum::middleware::from_fn_with_state(
                    rate_limiter,
                    rate_limit_middleware,
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
    info!(
        "rate limit: {}",
        if rate_config.is_enabled() {
            format!("{} rps, burst {}", rate_config.rps, rate_config.burst)
        } else {
            "disabled".to_string()
        }
    );
    info!(
        "max upload: {}",
        if state.max_upload_bytes == 0 {
            "unlimited".to_string()
        } else {
            format!("{} bytes", state.max_upload_bytes)
        }
    );

    let listener = tokio::net::TcpListener::bind(addr).await?;

    // Graceful shutdown on SIGINT/SIGTERM.
    let graceful = async {
        common::shutdown::signal().await;
        info!("received shutdown signal, shutting down gracefully");
    };

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(graceful)
    .await
    .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;

    Ok(())
}

pub fn router(state: Arc<AppState>) -> Router {
    routes::routes(state)
}
