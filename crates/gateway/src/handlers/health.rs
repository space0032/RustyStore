//! Health and metrics handlers.
//! GET /api/v1/health
//! GET /metrics

use axum::body::Body;
use axum::http::header;
use axum::response::{IntoResponse, Json, Response};
use serde::Serialize;

#[derive(Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub service: String,
}

pub async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".to_string(),
        service: "gateway".to_string(),
    })
}

/// Prometheus scrape endpoint.
pub async fn metrics() -> Response {
    let body = observability::prometheus_metrics();
    (
        [(
            header::CONTENT_TYPE,
            "text/plain; version=0.0.4; charset=utf-8",
        )],
        Body::from(body),
    )
        .into_response()
}
