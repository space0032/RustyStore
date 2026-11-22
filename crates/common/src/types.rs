//! Shared domain types.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A unique identifier for an uploaded file.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, sqlx::Type)]
#[sqlx(transparent)]
pub struct FileId(pub Uuid);

/// A unique identifier for a chunk.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, sqlx::Type)]
#[sqlx(transparent)]
pub struct ChunkId(pub Uuid);

/// A unique identifier for a storage node.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, sqlx::Type)]
#[sqlx(transparent)]
pub struct NodeId(pub Uuid);

/// Status of an upload session.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum UploadSessionStatus {
    Pending,
    Committed,
    Aborted,
}

/// Status of a file manifest.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum FileStatus {
    Uploading,
    Committed,
    Aborted,
}

/// Status of a replication task.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReplicationTaskStatus {
    Pending,
    InProgress,
    Done,
    Failed,
}

impl From<Uuid> for FileId {
    fn from(u: Uuid) -> Self {
        Self(u)
    }
}

impl From<Uuid> for ChunkId {
    fn from(u: Uuid) -> Self {
        Self(u)
    }
}

impl From<Uuid> for NodeId {
    fn from(u: Uuid) -> Self {
        Self(u)
    }
}
