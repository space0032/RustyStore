//! Authentication middleware.
//!
//! Validates API keys from the `Authorization: Bearer <token>` header.
//! In production, tokens should be validated against a JWT issuer or
//! database-backed token store.

use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::Response,
};
use std::sync::Arc;
use tracing::warn;

/// Authentication configuration.
#[derive(Clone, Debug)]
pub struct AuthConfig {
    /// Required API key. If None, authentication is disabled.
    pub api_key: Option<String>,
    /// Whether auth is required for all routes or just write routes.
    pub require_auth: bool,
}

impl AuthConfig {
    pub fn disabled() -> Self {
        Self {
            api_key: None,
            require_auth: false,
        }
    }

    pub fn enabled(api_key: String) -> Self {
        Self {
            api_key: Some(api_key),
            require_auth: true,
        }
    }

    pub fn from_env() -> Self {
        let api_key = std::env::var("API_KEY").ok();
        let require_auth = std::env::var("REQUIRE_AUTH")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(false);

        match api_key {
            Some(key) if require_auth => Self::enabled(key),
            _ => Self::disabled(),
        }
    }
}

/// Auth middleware that validates Bearer tokens.
pub async fn auth_middleware(
    State(config): State<Arc<AuthConfig>>,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    // If auth is disabled, pass through.
    if !config.require_auth {
        return Ok(next.run(request).await);
    }

    // Health, readiness, and metrics endpoints are always public.
    let path = request.uri().path();
    if path == "/api/v1/health" || path == "/api/v1/ready" || path == "/metrics" {
        return Ok(next.run(request).await);
    }

    // Extract the Authorization header.
    let auth_header = request
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok());

    match auth_header {
        Some(header) if header.starts_with("Bearer ") => {
            let token = &header[7..];
            match &config.api_key {
                Some(expected_key) if token == expected_key => Ok(next.run(request).await),
                _ => {
                    warn!("Invalid API key provided");
                    Err(StatusCode::UNAUTHORIZED)
                }
            }
        }
        _ => {
            warn!("Missing or invalid Authorization header");
            Err(StatusCode::UNAUTHORIZED)
        }
    }
}
