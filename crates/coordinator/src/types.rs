//! Shared types for the coordinator client.

use uuid::Uuid;

/// A chunk manifest entry used by the gateway.
#[derive(Clone, Debug)]
pub struct ChunkManifestEntry {
    pub chunk_id: Uuid,
    pub chunk_index: i64,
    pub size_bytes: i64,
    pub sha256_hex: String,
    /// Nodes that durably stored this chunk; recorded as replicas on commit.
    pub node_ids: Vec<Uuid>,
}

/// A chunk location entry from the manifest.
#[derive(Clone, Debug)]
pub struct ChunkLocationEntry {
    pub chunk_id: Uuid,
    pub chunk_index: i64,
    pub size_bytes: i64,
    pub sha256_hex: String,
    pub node_ids: Vec<Uuid>,
}

/// The full file manifest.
#[derive(Clone, Debug)]
pub struct Manifest {
    pub file_id: Uuid,
    pub filename: String,
    pub total_size: i64,
    pub chunk_size: i64,
    pub chunk_count: i32,
    pub replication_factor: i32,
    pub chunks: Vec<ChunkLocationEntry>,
}

/// Node info entry.
#[derive(Clone, Debug)]
pub struct NodeInfoEntry {
    pub node_id: Uuid,
    pub addr: String,
    pub grpc_addr: String,
    pub healthy: bool,
    pub available_bytes: i64,
    pub capacity_bytes: i64,
}

/// Cluster stats.
#[derive(Clone, Debug)]
pub struct ClusterStats {
    pub total_nodes: i32,
    pub healthy_nodes: i32,
    pub total_capacity_bytes: i64,
    pub total_available_bytes: i64,
    pub total_files: i64,
    pub total_chunks: i64,
    pub under_replicated_chunks: i64,
}
