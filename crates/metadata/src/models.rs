//! Metadata row types (mirror the PostgreSQL schema).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

/// A registered storage node.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct StorageNodeRow {
    pub node_id: Uuid,
    pub addr: String,
    pub grpc_addr: String,
    pub capacity_bytes: i64,
    pub available_bytes: i64,
    pub used_bytes: i64,
    pub lease_expiry: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,
    pub healthy: bool,
    pub version: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A file manifest (committed files only are readable).
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct FileRow {
    pub file_id: Uuid,
    pub filename: String,
    pub total_size: i64,
    pub chunk_size: i64,
    pub chunk_count: i32,
    pub replication_factor: i32,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A content-addressed chunk.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ChunkRow {
    pub chunk_id: Uuid,
    pub sha256_hex: String,
    pub size_bytes: i64,
    pub chunk_index: i32,
    pub file_id: Uuid,
}

/// A chunk replica placement.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ChunkReplicaRow {
    pub replica_id: Uuid,
    pub chunk_id: Uuid,
    pub node_id: Uuid,
    pub durable: bool,
    pub created_at: DateTime<Utc>,
}

/// An upload session (tracks partial uploads for abort cleanup).
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct UploadSessionRow {
    pub upload_id: Uuid,
    pub file_id: Option<Uuid>,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

/// A replication task (recovery work queue).
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ReplicationTaskRow {
    pub task_id: Uuid,
    pub chunk_id: Uuid,
    pub source_node_id: Option<Uuid>,
    pub target_node_id: Uuid,
    pub status: String,
    pub attempts: i32,
    pub max_attempts: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
