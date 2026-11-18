//! Storage node state and operations.
//!
//! Wraps the storage engine (FsChunkStore) with node identity. Provides chunk
//! store/retrieve/delete operations used by the gRPC service.

use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use storage_engine::{ChunkMeta, ChunkStore, FsChunkStore, StoreResult};
use uuid::Uuid;

/// Inner state of a storage node, wrapped in Arc for sharing across gRPC
/// service instances.
#[derive(Clone)]
pub struct StorageNode {
    inner: Arc<StorageNodeInner>,
}

/// Internal node state.
pub(crate) struct StorageNodeInner {
    pub node_id: RwLock<Option<Uuid>>,
    pub store: FsChunkStore,
}

impl StorageNode {
    /// Create a new storage node with the given storage path.
    pub async fn new(storage_path: impl Into<PathBuf>) -> StoreResult<Self> {
        let store = FsChunkStore::new(storage_path);
        store.init().await?;

        Ok(Self {
            inner: Arc::new(StorageNodeInner {
                node_id: RwLock::new(None),
                store,
            }),
        })
    }

    /// Get the current node ID, if registered.
    pub fn node_id(&self) -> Option<Uuid> {
        *self.inner.node_id.read().unwrap()
    }

    /// Adopt the node ID assigned by the coordinator during registration.
    pub fn set_node_id(&self, node_id: Uuid) {
        *self.inner.node_id.write().unwrap() = Some(node_id);
    }

    /// Query the current disk capacity of the storage volume.
    pub async fn capacity(&self) -> StoreResult<storage_engine::CapacityInfo> {
        self.inner.store.get_capacity().await
    }

    /// Store a chunk atomically (temp file -> fsync -> rename).
    /// Verifies SHA-256 before exposing the chunk to readers.
    pub async fn store_chunk(
        &self,
        chunk_id: &str,
        data: &[u8],
        expected_sha256: &str,
    ) -> StoreResult<()> {
        self.inner
            .store
            .store_chunk(chunk_id, data, expected_sha256)
            .await
    }

    /// Open a streaming reader for a chunk.
    pub async fn open_chunk_reader(
        &self,
        chunk_id: &str,
    ) -> StoreResult<(ChunkMeta, Box<dyn tokio::io::AsyncRead + Unpin + Send>)> {
        self.inner.store.open_chunk_reader(chunk_id).await
    }

    /// Delete a chunk by id. Returns false if the chunk did not exist.
    pub async fn delete_chunk(&self, chunk_id: &str) -> StoreResult<bool> {
        self.inner.store.delete_chunk(chunk_id).await
    }

    /// List stored chunks (metadata only), excluding in-progress temp files.
    pub async fn list_chunks(&self) -> StoreResult<Vec<storage_engine::StoredChunkInfo>> {
        self.inner.store.list_chunks().await
    }

    /// Get the storage path.
    pub fn storage_path(&self) -> &std::path::Path {
        self.inner.store.storage_path()
    }
}
