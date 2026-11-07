//! Gateway error type.

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use std::fmt;

#[derive(Debug)]
pub enum GatewayError {
    NotFound,
    InvalidArgument(String),
    Internal(String),
    Grpc(String),
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

impl IntoResponse for GatewayError {
    fn into_response(self) -> Response {
        let (status, body) = match self {
            GatewayError::NotFound => (StatusCode::NOT_FOUND, "not found"),
            GatewayError::InvalidArgument(_) => (StatusCode::BAD_REQUEST, "invalid argument"),
            GatewayError::Internal(_) => (StatusCode::INTERNAL_SERVER_ERROR, "internal error"),
            GatewayError::Grpc(_) => (StatusCode::BAD_GATEWAY, "grpc error"),
        };
        (status, body.to_string()).into_response()
    }
}
