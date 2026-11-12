//! Storage Node gRPC server.
//!
//! Exposes NodeService, ChunkService, and ClusterService. Persists chunks via
//! the storage engine (atomic writes + SHA-256), reports capacity, and
//! handles replication transfers.

mod node;
mod server;
mod service;

pub use node::StorageNode;
pub use server::StorageNodeServer;
