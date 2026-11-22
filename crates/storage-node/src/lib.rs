//! Storage Node gRPC server.
//!
//! Exposes ChunkService. Persists chunks via the storage engine (atomic writes
//! + SHA-256) and handles replication transfers.

mod node;
mod server;
mod service;

pub mod http;

pub use node::StorageNode;
pub use server::StorageNodeServer;
