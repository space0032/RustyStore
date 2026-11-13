//! Gateway error type.
//!
//! Errors are rendered as structured JSON with a stable machine-readable
//! `code` plus a human-readable `message`:
//!
//! ```json
//! { "error": { "code": "not_found", "message": "not found" } }
//! ```

use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;
use std::fmt;

#[derive(Debug)]
pub enum GatewayError {
    NotFound,
    InvalidArgument(String),
    Internal(String),
    Grpc(String),
}

impl GatewayError {
    /// HTTP status code for this error.
    pub fn status(&self) -> StatusCode {
        match self {
            GatewayError::NotFound => StatusCode::NOT_FOUND,
            GatewayError::InvalidArgument(_) => StatusCode::BAD_REQUEST,
            GatewayError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
            GatewayError::Grpc(_) => StatusCode::BAD_GATEWAY,
        }
    }

    /// Stable, machine-readable error code.
    pub fn code(&self) -> &'static str {
        match self {
            GatewayError::NotFound => "not_found",
            GatewayError::InvalidArgument(_) => "invalid_argument",
            GatewayError::Internal(_) => "internal",
            GatewayError::Grpc(_) => "upstream_error",
        }
    }
}

impl fmt::Display for GatewayError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GatewayError::NotFound => write!(f, "not found"),
            GatewayError::InvalidArgument(s) => write!(f, "invalid argument: {}", s),
            GatewayError::Internal(s) => write!(f, "internal: {}", s),
            GatewayError::Grpc(s) => write!(f, "grpc: {}", s),
        }
    }
}

impl std::error::Error for GatewayError {}

#[derive(Serialize)]
struct ErrorDetail {
    code: &'static str,
    message: String,
}

#[derive(Serialize)]
struct ErrorBody {
    error: ErrorDetail,
}

impl IntoResponse for GatewayError {
    fn into_response(self) -> Response {
        let status = self.status();
        let body = ErrorBody {
            error: ErrorDetail {
                code: self.code(),
                message: self.to_string(),
            },
        };
        (status, Json(body)).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;

    async fn render(err: GatewayError) -> (StatusCode, String) {
        let resp = err.into_response();
        let status = resp.status();
        let bytes = to_bytes(resp.into_body(), usize::MAX)
            .await
            .expect("read body");
        let body = String::from_utf8(bytes.to_vec()).expect("utf-8 body");
        (status, body)
    }

    #[tokio::test]
    async fn not_found_is_404_json() {
        let (status, body) = render(GatewayError::NotFound).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert!(body.contains("\"code\":\"not_found\""), "body: {body}");
    }

    #[tokio::test]
    async fn invalid_argument_is_400_json() {
        let (status, body) = render(GatewayError::InvalidArgument("bad id".into())).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(
            body.contains("\"code\":\"invalid_argument\""),
            "body: {body}"
        );
        assert!(body.contains("bad id"), "body: {body}");
    }

    #[tokio::test]
    async fn internal_is_500_and_grpc_is_502() {
        let (s1, b1) = render(GatewayError::Internal("boom".into())).await;
        assert_eq!(s1, StatusCode::INTERNAL_SERVER_ERROR);
        assert!(b1.contains("\"code\":\"internal\""), "body: {b1}");

        let (s2, b2) = render(GatewayError::Grpc("unavailable".into())).await;
        assert_eq!(s2, StatusCode::BAD_GATEWAY);
        assert!(b2.contains("\"code\":\"upstream_error\""), "body: {b2}");
    }
}
