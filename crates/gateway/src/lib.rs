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
mod grpc;
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
    coordinator_addrs: Vec<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    let max_upload_bytes: u64 = common::config::parse_env("MAX_UPLOAD_BYTES", 0)?;
    let state = Arc::new(
        AppState::with_coordinator_addrs(coordinator_addrs.clone())
            .with_max_upload_bytes(max_upload_bytes),
    );

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

    info!(
        "gateway listening on {addr}, coordinators at {}",
        coordinator_addrs.join(", ")
    );
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

/// Normalize a storage-node address into a gRPC endpoint, tolerating
/// addresses that already carry an `http(s)://` scheme.
pub(crate) fn grpc_endpoint(addr: &str) -> String {
    if addr.starts_with("http://") || addr.starts_with("https://") {
        addr.to_string()
    } else {
        format!("http://{addr}")
    }
}

/// Parse `COORDINATOR_ADDRS` (comma-separated) with `COORDINATOR_ADDR` as the
/// single-address fallback. Empty entries are ignored; an all-empty value is
/// a startup error.
pub fn parse_coordinator_addrs(
    raw: &str,
    fallback: &str,
) -> Result<Vec<String>, common::config::ConfigError> {
    let source = if raw.trim().is_empty() { fallback } else { raw };
    let addrs: Vec<String> = source
        .split(',')
        .map(str::trim)
        .filter(|addr| !addr.is_empty())
        .map(grpc_endpoint)
        .collect();
    if addrs.is_empty() {
        return Err(common::config::ConfigError::Empty {
            key: "COORDINATOR_ADDRS".to_string(),
        });
    }
    Ok(addrs)
}

/// Read the coordinator address list from the environment.
pub fn coordinator_addrs_from_env() -> Result<Vec<String>, common::config::ConfigError> {
    let raw = common::config::env_string("COORDINATOR_ADDRS", "");
    let fallback = common::config::env_string("COORDINATOR_ADDR", "127.0.0.1:50051");
    parse_coordinator_addrs(&raw, &fallback)
}

#[cfg(test)]
mod endpoint_tests {
    use super::{grpc_endpoint, parse_coordinator_addrs};

    #[test]
    fn adds_scheme_only_when_missing() {
        assert_eq!(grpc_endpoint("node:50051"), "http://node:50051");
        assert_eq!(grpc_endpoint("http://node:50051"), "http://node:50051");
        assert_eq!(grpc_endpoint("https://node:50051"), "https://node:50051");
    }

    #[test]
    fn parses_multiple_coordinators_and_normalizes_schemes() {
        let addrs =
            parse_coordinator_addrs(" coordinator-1:50051, http://coordinator-2:50051 ", "")
                .unwrap();
        assert_eq!(
            addrs,
            vec![
                "http://coordinator-1:50051".to_string(),
                "http://coordinator-2:50051".to_string()
            ]
        );
    }

    #[test]
    fn falls_back_to_single_coordinator_addr() {
        assert_eq!(
            parse_coordinator_addrs("", "coordinator:50051").unwrap(),
            vec!["http://coordinator:50051".to_string()]
        );
    }

    #[test]
    fn rejects_an_empty_address_list() {
        assert!(parse_coordinator_addrs(" , ", "").is_err());
    }
}
