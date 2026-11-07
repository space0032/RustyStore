//! PostgreSQL metadata layer.
//!
//! Defines sqlx-backed models and queries for storage nodes, files, chunks,
//! chunk replicas, upload sessions, and replication tasks. All metadata
//! mutations run inside transactions to avoid marking incomplete uploads as
//! committed.

pub mod models;
pub mod queries;

pub use models::*;
pub use queries::*;
