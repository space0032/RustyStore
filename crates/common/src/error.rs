//! Domain error type.
//!
//! Library/domain errors use `thiserror`. Application boundaries (main,
//! HTTP handlers, gRPC services) convert to `anyhow` where appropriate.

use thiserror::Error;

#[derive(Error, Debug)]
pub enum RustyStoreError {
    #[error("not found: {0}")]
    NotFound(String),

    #[error("invalid argument: {0}")]
    InvalidArgument(String),

    #[error("storage error: {0}")]
    Storage(String),

    #[error("integrity check failed: expected={expected} actual={actual}")]
    IntegrityMismatch { expected: String, actual: String },

    #[error("node unavailable: {node_id}")]
    NodeUnavailable { node_id: String },

    #[error("under replicated: chunk={chunk_id} required={required} actual={actual}")]
    UnderReplicated {
        chunk_id: String,
        required: i32,
        actual: i32,
    },

    #[error("insufficient nodes: required={required} available={available}")]
    InsufficientNodes { required: i32, available: i32 },

    #[error("database error: {0}")]
    Database(String),

    #[error("internal error: {0}")]
    Internal(String),

    #[error("grpc transport error: {0}")]
    Grpc(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}
