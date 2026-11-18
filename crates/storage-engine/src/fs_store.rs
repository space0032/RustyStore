//! Filesystem-backed chunk store.
//!
//! Chunks are laid out as `<storage_path>/<chunk_id>`. Writes use a sibling
//! temp file (`<chunk_id>.tmp.<pid>`) that is fsynced and then atomically
//! renamed over the final path. Readers never observe a partially written
//! chunk. Blocking filesystem calls are dispatched to `spawn_blocking` to
//! avoid starving the Tokio runtime.

use crate::chunk_store::{
    CapacityInfo, ChunkEntry, ChunkMeta, ChunkStore, StoreResult, StoredChunkInfo,
};
use async_trait::async_trait;
use common::error::RustyStoreError;
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::{Path, PathBuf};
use tokio::fs;
use tokio::task::spawn_blocking;

/// Filesystem chunk store.
#[derive(Clone)]
pub struct FsChunkStore {
    storage_path: PathBuf,
}

impl FsChunkStore {
    pub fn new(storage_path: impl Into<PathBuf>) -> Self {
        Self {
            storage_path: storage_path.into(),
        }
    }

    /// Ensure the storage directory exists.
    pub async fn init(&self) -> StoreResult<()> {
        let path = self.storage_path.clone();
        spawn_blocking(move || std::fs::create_dir_all(&path).map_err(RustyStoreError::Io))
            .await
            .map_err(|e| RustyStoreError::Internal(e.to_string()))?
    }

    pub fn chunk_path(&self, chunk_id: &str) -> PathBuf {
        self.storage_path.join(chunk_id)
    }

    pub fn temp_path(&self, chunk_id: &str) -> PathBuf {
        self.storage_path
            .join(format!("{}.tmp.{}", chunk_id, std::process::id()))
    }

    /// Compute SHA-256 hex of a byte slice.
    pub fn sha256_hex(data: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(data);
        hex::encode(hasher.finalize())
    }

    /// Compute SHA-256 hex by reading a file (blocking, runs on spawn_blocking).
    async fn sha256_hex_file(path: &Path) -> StoreResult<String> {
        let path = path.to_path_buf();
        spawn_blocking(move || {
            let mut file = std::fs::File::open(&path).map_err(RustyStoreError::Io)?;
            let mut hasher = Sha256::new();
            let mut buf = [0u8; 64 * 1024];
            loop {
                let n = file.read(&mut buf).map_err(RustyStoreError::Io)?;
                if n == 0 {
                    break;
                }
                hasher.update(&buf[..n]);
            }
            Ok(hex::encode(hasher.finalize()))
        })
        .await
        .map_err(|e| RustyStoreError::Internal(e.to_string()))?
    }

    /// Read entire file into memory (blocking, runs on spawn_blocking).
    async fn read_file(path: &Path) -> StoreResult<Vec<u8>> {
        let path = path.to_path_buf();
        spawn_blocking(move || std::fs::read(&path).map_err(RustyStoreError::Io))
            .await
            .map_err(|e| RustyStoreError::Internal(e.to_string()))?
    }

    /// Best-effort disk capacity query.
    /// Returns zeroes when the platform does not expose capacity info.
    fn disk_capacity(_path: &Path) -> CapacityInfo {
        CapacityInfo {
            total_bytes: 0,
            available_bytes: 0,
            used_bytes: 0,
        }
    }

    /// Check if a path exists (blocking, runs on spawn_blocking).
    async fn path_exists(path: &Path) -> StoreResult<bool> {
        let path = path.to_path_buf();
        let exists = spawn_blocking(move || path.metadata().is_ok())
            .await
            .map_err(|e| RustyStoreError::Internal(e.to_string()))?;
        Ok(exists)
    }
}

#[async_trait]
impl ChunkStore for FsChunkStore {
    async fn store_chunk(
        &self,
        chunk_id: &str,
        data: &[u8],
        expected_sha256: &str,
    ) -> StoreResult<()> {
        let actual = Self::sha256_hex(data);
        if actual != expected_sha256 {
            return Err(RustyStoreError::IntegrityMismatch {
                expected: expected_sha256.to_string(),
                actual,
            });
        }

        let temp_path = self.temp_path(chunk_id);
        let final_path = self.chunk_path(chunk_id);

        // Atomic write: temp file → write → fsync → rename.
        let data = data.to_vec();
        let temp_path_owned = temp_path.clone();
        spawn_blocking(move || {
            use std::io::Write;
            let mut file = std::fs::File::create(&temp_path_owned).map_err(RustyStoreError::Io)?;
            file.write_all(&data).map_err(RustyStoreError::Io)?;
            file.sync_all().map_err(RustyStoreError::Io)?;
            drop(file);
            Ok::<(), RustyStoreError>(())
        })
        .await
        .map_err(|e| RustyStoreError::Internal(e.to_string()))??;

        // Atomic rename: readers never see a partial chunk.
        let temp_path_owned2 = temp_path.clone();
        let final_path_owned = final_path.clone();
        spawn_blocking(move || {
            std::fs::rename(&temp_path_owned2, &final_path_owned).map_err(RustyStoreError::Io)
        })
        .await
        .map_err(|e| RustyStoreError::Internal(e.to_string()))??;

        Ok(())
    }

    async fn retrieve_chunk(&self, chunk_id: &str) -> StoreResult<ChunkEntry> {
        let path = self.chunk_path(chunk_id);

        if !Self::path_exists(&path).await? {
            return Err(RustyStoreError::NotFound(format!(
                "chunk {chunk_id} not found"
            )));
        }

        let data: Vec<u8> = Self::read_file(&path).await?;
        let actual = Self::sha256_hex(&data);

        Ok(ChunkEntry {
            chunk_id: chunk_id.to_string(),
            sha256_hex: actual,
            size_bytes: data.len() as u64,
            data,
        })
    }

    async fn open_chunk_reader(
        &self,
        chunk_id: &str,
    ) -> StoreResult<(ChunkMeta, Box<dyn tokio::io::AsyncRead + Unpin + Send>)> {
        let path = self.chunk_path(chunk_id);

        if !Self::path_exists(&path).await? {
            return Err(RustyStoreError::NotFound(format!(
                "chunk {chunk_id} not found"
            )));
        }

        let metadata: std::fs::Metadata = {
            let path = path.clone();
            spawn_blocking(move || path.metadata().map_err(RustyStoreError::Io))
                .await
                .map_err(|e| RustyStoreError::Internal(e.to_string()))??
        };

        let file = fs::File::open(&path).await.map_err(RustyStoreError::Io)?;

        let meta = ChunkMeta {
            chunk_id: chunk_id.to_string(),
            sha256_hex: String::new(),
            size_bytes: metadata.len(),
        };

        Ok((meta, Box::new(file)))
    }

    async fn verify_chunk(&self, chunk_id: &str, expected_sha256: &str) -> StoreResult<()> {
        let path = self.chunk_path(chunk_id);
        let actual = Self::sha256_hex_file(&path).await?;
        if actual != expected_sha256 {
            return Err(RustyStoreError::IntegrityMismatch {
                expected: expected_sha256.to_string(),
                actual,
            });
        }
        Ok(())
    }

    async fn delete_chunk(&self, chunk_id: &str) -> StoreResult<bool> {
        let path = self.chunk_path(chunk_id);
        let path = path.clone();
        let result =
            spawn_blocking(move || std::fs::remove_file(&path).map_err(RustyStoreError::Io))
                .await
                .map_err(|e| RustyStoreError::Internal(e.to_string()))?;
        match result {
            Ok(()) => Ok(true),
            Err(RustyStoreError::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(e),
        }
    }

    async fn chunk_exists(&self, chunk_id: &str) -> StoreResult<bool> {
        let path = self.chunk_path(chunk_id);
        Self::path_exists(&path).await
    }

    async fn list_chunks(&self) -> StoreResult<Vec<StoredChunkInfo>> {
        let dir = self.storage_path.clone();
        spawn_blocking(move || {
            use std::time::UNIX_EPOCH;

            let entries = std::fs::read_dir(&dir).map_err(RustyStoreError::Io)?;
            let mut chunks = Vec::new();
            for entry in entries {
                let entry = entry.map_err(RustyStoreError::Io)?;
                let file_type = entry.file_type().map_err(RustyStoreError::Io)?;
                if !file_type.is_file() {
                    continue;
                }
                let name = entry.file_name();
                let Some(chunk_id) = name.to_str() else {
                    continue;
                };
                // Skip in-progress writes (`<chunk_id>.tmp.<pid>`).
                if chunk_id.contains(".tmp.") {
                    continue;
                }
                let metadata = entry.metadata().map_err(RustyStoreError::Io)?;
                let modified_epoch_ms = metadata
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map(|d| d.as_millis() as i64)
                    .unwrap_or(0);
                chunks.push(StoredChunkInfo {
                    chunk_id: chunk_id.to_string(),
                    size_bytes: metadata.len(),
                    modified_epoch_ms,
                });
            }
            chunks.sort_by(|a, b| a.chunk_id.cmp(&b.chunk_id));
            Ok::<_, RustyStoreError>(chunks)
        })
        .await
        .map_err(|e| RustyStoreError::Internal(e.to_string()))?
    }

    async fn get_capacity(&self) -> StoreResult<CapacityInfo> {
        let path = self.storage_path.clone();
        let info = spawn_blocking(move || Self::disk_capacity(&path))
            .await
            .map_err(|e| RustyStoreError::Internal(e.to_string()))?;
        Ok(info)
    }

    fn storage_path(&self) -> &Path {
        &self.storage_path
    }
}
