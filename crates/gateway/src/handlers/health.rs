//! Health, readiness, and metrics handlers.
//! GET /api/v1/health
//! GET /api/v1/ready
//! GET /metrics

use axum::body::Body;
use axum::extract::State;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Json, Response};
use serde::Serialize;
use std::sync::Arc;
use std::time::Duration;

use crate::AppState;

const READINESS_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub service: String,
}

#[derive(Serialize)]
pub struct ReadyResponse {
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

pub async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".to_string(),
        service: "gateway".to_string(),
    })
}

/// Readiness probe: reports ready only when the coordinator is reachable.
pub async fn ready(State(state): State<Arc<AppState>>) -> Response {
    let probe = async {
        let mut client = state.coord_client().await.map_err(|_| ())?;
        client.list_nodes().await.map_err(|_| ())
    };

    match tokio::time::timeout(READINESS_TIMEOUT, probe).await {
        Ok(Ok(_)) => Json(ReadyResponse {
            status: "ready".to_string(),
            reason: None,
        })
        .into_response(),
        Ok(Err(())) | Err(_) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ReadyResponse {
                status: "not_ready".to_string(),
                reason: Some("coordinator unavailable".to_string()),
            }),
        )
            .into_response(),
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AppState;
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    #[test]
    fn ready_response_shape() {
        let ready = ReadyResponse {
            status: "ready".to_string(),
            reason: None,
        };
        let json = serde_json::to_string(&ready).expect("serialize");
        assert_eq!(json, r#"{"status":"ready"}"#);

        let not_ready = ReadyResponse {
            status: "not_ready".to_string(),
            reason: Some("coordinator unavailable".to_string()),
        };
        let json = serde_json::to_string(&not_ready).expect("serialize");
        assert!(json.contains("\"reason\":\"coordinator unavailable\""));
    }

    #[tokio::test]
    async fn ready_is_503_when_coordinator_unreachable() {
        let state = Arc::new(AppState::new("http://127.0.0.1:1"));
        let app = crate::router(state);
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/ready")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("response");
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    }
}
