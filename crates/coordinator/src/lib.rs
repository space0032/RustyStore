//! Cluster Coordinator.
//!
//! Maintains cluster metadata, node health/leases, placement decisions, file
//! manifests, and replication scheduling. Runs a Tonic gRPC service for
//! storage-node communication and an in-process recovery scheduling.

mod client;
mod placement;
mod server;
mod service;
pub mod types;

pub use client::CoordinatorClient;
pub use placement::PlacementResult;
pub use placement::place_replicas;
pub use server::CoordinatorServer;
pub use service::CoordinatorService;
pub use types::{ChunkLocationEntry, ChunkManifestEntry, ClusterStats, Manifest, NodeInfoEntry};
