//! Abstraction over a chunk store.
//!
//! A chunk store persists and retrieves opaque byte blobs identified by a
//! content hash. Implementations may be filesystem-backed (now) or
//! object-storage-backed (later).
//!
//! Chunks are bounded in size (4–8 MB), so buffering a single chunk in
//! memory is acceptable. Entire files (which may be gigabytes) must never
//! be buffered — that constraint is enforced at the gateway layer.

use async_trait::async_trait;
use common::error::RustyStoreError;
use std::path::Path;
use tokio::io::AsyncRead;

/// Result type for storage-engine operations.
pub type StoreResult<T> = Result<T, RustyStoreError>;

/// Metadata for a stored chunk (no payload).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChunkMeta {
    pub chunk_id: String,
    pub sha256_hex: String,
    pub size_bytes: u64,
}

/// A chunk with its payload. Only used for bounded chunks (≤ 8 MB).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChunkEntry {
    pub chunk_id: String,
    pub sha256_hex: String,
    pub size_bytes: u64,
    pub data: Vec<u8>,
}

/// Disk capacity information for a storage node.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapacityInfo {
    pub total_bytes: u64,
    pub available_bytes: u64,
    pub used_bytes: u64,
}

/// A chunk discovered by listing the store (no payload).
///
/// `modified_epoch_ms` is the last-modification time in Unix epoch
/// milliseconds; the coordinator uses it to avoid reclaiming chunks that may
/// still belong to an in-flight upload.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoredChunkInfo {
    pub chunk_id: String,
    pub size_bytes: u64,
    pub modified_epoch_ms: i64,
}

#[async_trait]
pub trait ChunkStore: Send + Sync {
    /// Store a chunk atomically (temp file → fsync → rename).
    /// Verifies SHA-256 before exposing the chunk to readers.
    async fn store_chunk(
        &self,
        chunk_id: &str,
        data: &[u8],
        expected_sha256: &str,
    ) -> StoreResult<()>;

    /// Retrieve a chunk by id. Returns the full payload (bounded ≤ 8 MB).
    /// Verifies SHA-256 on read; returns IntegrityMismatch on corruption.
    async fn retrieve_chunk(&self, chunk_id: &str) -> StoreResult<ChunkEntry>;

    /// Open a streaming reader for a chunk. Caller reads incrementally;
    /// hash verification is the caller's responsibility via `verify_chunk`.
    async fn open_chunk_reader(
        &self,
        chunk_id: &str,
    ) -> StoreResult<(ChunkMeta, Box<dyn AsyncRead + Unpin + Send>)>;

    /// Verify the SHA-256 of a chunk file on disk.
    async fn verify_chunk(&self, chunk_id: &str, expected_sha256: &str) -> StoreResult<()>;

    /// Delete a chunk by id. Returns false if the chunk did not exist.
    async fn delete_chunk(&self, chunk_id: &str) -> StoreResult<bool>;

    /// Check whether a chunk exists.
    async fn chunk_exists(&self, chunk_id: &str) -> StoreResult<bool>;

    /// List all chunks currently stored (metadata only).
    ///
    /// Used by the coordinator's orphan-chunk garbage collector. Temporary
    /// files from in-progress writes must not be reported.
    async fn list_chunks(&self) -> StoreResult<Vec<StoredChunkInfo>>;

    /// Return disk capacity for the storage volume.
    async fn get_capacity(&self) -> StoreResult<CapacityInfo>;

    /// Return the directory holding chunks.
    fn storage_path(&self) -> &Path;
}
