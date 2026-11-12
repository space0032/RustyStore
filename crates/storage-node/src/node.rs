//! Storage node state and operations.
//!
//! Wraps the storage engine (FsChunkStore) with node identity and address
//! information. Provides chunk store/retrieve/delete operations used by the
//! gRPC service.

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
    pub addr: RwLock<String>,
    pub grpc_addr: RwLock<String>,
    pub store: FsChunkStore,
}

impl StorageNode {
    /// Create a new storage node with the given storage path.
    pub async fn new(
        addr: String,
        grpc_addr: String,
        storage_path: impl Into<PathBuf>,
    ) -> StoreResult<Self> {
        let store = FsChunkStore::new(storage_path);
        store.init().await?;

        Ok(Self {
            inner: Arc::new(StorageNodeInner {
                node_id: RwLock::new(None),
                addr: RwLock::new(addr),
                grpc_addr: RwLock::new(grpc_addr),
                store,
            }),
        })
    }

    /// Register this node: generate a node ID and ensure the store directory
    /// exists. Returns the assigned node ID.
    pub async fn register(
        &self,
        addr: &str,
        grpc_addr: &str,
        _capacity_bytes: i64,
    ) -> StoreResult<Uuid> {
        let node_id = Uuid::new_v4();

        // Update stored address info if provided.
        if !addr.is_empty() {
            *self.inner.addr.write().unwrap() = addr.to_string();
        }
        if !grpc_addr.is_empty() {
            *self.inner.grpc_addr.write().unwrap() = grpc_addr.to_string();
        }

        *self.inner.node_id.write().unwrap() = Some(node_id);

        // Ensure storage directory exists.
        self.inner.store.init().await?;

        Ok(node_id)
    }

    /// Get the current node ID, if registered.
    pub fn node_id(&self) -> Option<Uuid> {
        *self.inner.node_id.read().unwrap()
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

    /// Get the storage path.
    pub fn storage_path(&self) -> &std::path::Path {
        self.inner.store.storage_path()
    }

    /// Get the node address.
    pub fn addr(&self) -> String {
        self.inner.addr.read().unwrap().clone()
    }

    /// Get the gRPC address.
    pub fn grpc_addr(&self) -> String {
        self.inner.grpc_addr.read().unwrap().clone()
    }
}
