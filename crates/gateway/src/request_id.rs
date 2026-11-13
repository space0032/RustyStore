//! Request correlation middleware.
//!
//! Ensures every request carries an `x-request-id`: the incoming value is
//! preserved when present, otherwise a UUID is generated. The identifier is
//! echoed on the response and attached to the request span so logs can be
//! correlated across the gateway and downstream calls.

use axum::{
    extract::Request,
    http::{HeaderName, HeaderValue},
    middleware::Next,
    response::Response,
};
use tracing::Instrument;
use uuid::Uuid;

/// Canonical request-id header (lowercase).
pub const REQUEST_ID_HEADER: &str = "x-request-id";

/// Choose the request id: reuse a non-empty incoming value, else generate one.
pub fn resolve_request_id(existing: Option<&HeaderValue>) -> String {
    existing
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| Uuid::new_v4().to_string())
}

fn header_value(id: &str) -> Option<HeaderValue> {
    HeaderValue::from_str(id).ok()
}

/// Middleware that assigns and echoes a request id.
pub async fn request_id_middleware(mut request: Request, next: Next) -> Response {
    let id = resolve_request_id(request.headers().get(REQUEST_ID_HEADER));

    let name = HeaderName::from_static(REQUEST_ID_HEADER);
    if let Some(value) = header_value(&id) {
        request.headers_mut().insert(name.clone(), value);
    }

    let span = tracing::info_span!("request", request_id = %id);
    let mut response = next.run(request).instrument(span).await;

    if let Some(value) = header_value(&id) {
        response.headers_mut().insert(name, value);
    }

    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Router, routing::get};
    use tower::ServiceExt;

    #[test]
    fn generates_uuid_when_absent() {
        let id = resolve_request_id(None);
        assert_eq!(id.len(), 36, "expected UUID: {id}");
        assert!(Uuid::parse_str(&id).is_ok());
    }

    #[test]
    fn preserves_incoming_id() {
        let value = HeaderValue::from_static("client-abc-123");
        assert_eq!(resolve_request_id(Some(&value)), "client-abc-123");
    }

    #[test]
    fn ignores_empty_incoming_id() {
        let value = HeaderValue::from_static("");
        let id = resolve_request_id(Some(&value));
        assert!(
            Uuid::parse_str(&id).is_ok(),
            "expected generated UUID: {id}"
        );
    }

    #[tokio::test]
    async fn echoes_request_id_on_response() {
        let app = Router::new()
            .route("/", get(|| async { "ok" }))
            .layer(axum::middleware::from_fn(request_id_middleware));

        let response = app
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .uri("/")
                    .header(REQUEST_ID_HEADER, "trace-42")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .expect("response");
        assert_eq!(
            response
                .headers()
                .get(REQUEST_ID_HEADER)
                .and_then(|v| v.to_str().ok()),
            Some("trace-42")
        );

        let generated = app
            .oneshot(
                axum::http::Request::builder()
                    .uri("/")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .expect("response");
        assert!(generated.headers().get(REQUEST_ID_HEADER).is_some());
    }
}
