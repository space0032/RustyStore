//! Local-filesystem chunk storage engine.
//!
//! Provides atomic, integrity-verified chunk persistence on top of the local
//! filesystem. Chunks are written to a temporary file, fsynced, verified
//! against their SHA-256 hash, then atomically renamed into place so that
//! readers never observe a partially written chunk.
//!
//! Blocking filesystem operations are dispatched to `spawn_blocking` to
//! avoid starving the Tokio runtime.

mod chunk_store;
mod fs_store;

#[cfg(feature = "s3")]
mod s3_store;

pub use chunk_store::{
    CapacityInfo, ChunkEntry, ChunkMeta, ChunkStore, StoreResult, StoredChunkInfo,
};
pub use fs_store::FsChunkStore;

#[cfg(feature = "s3")]
pub use s3_store::S3ChunkStore;

#[cfg(test)]
mod tests {
    use super::*;
    use common::error::RustyStoreError;
    use tokio::io::AsyncReadExt;

    async fn test_store() -> (FsChunkStore, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let store = FsChunkStore::new(dir.path());
        store.init().await.unwrap();
        (store, dir)
    }

    #[tokio::test]
    async fn test_store_and_retrieve_chunk() {
        let (store, _dir) = test_store().await;
        let chunk_id = "test-chunk-1";
        let data = b"hello world, this is a test chunk payload";
        let hash = FsChunkStore::sha256_hex(data);

        store
            .store_chunk(chunk_id, data, &hash)
            .await
            .expect("store should succeed");

        let entry = store
            .retrieve_chunk(chunk_id)
            .await
            .expect("retrieve should succeed");
        assert_eq!(entry.data, data);
        assert_eq!(entry.sha256_hex, hash);
        assert_eq!(entry.size_bytes, data.len() as u64);
        assert!(store.chunk_exists(chunk_id).await.unwrap());
    }

    #[tokio::test]
    async fn test_retrieve_nonexistent_chunk() {
        let (store, _dir) = test_store().await;
        let err = store
            .retrieve_chunk("does-not-exist")
            .await
            .expect_err("should error");
        assert!(matches!(err, RustyStoreError::NotFound(_)));
    }

    #[tokio::test]
    async fn test_integrity_mismatch_on_store() {
        let (store, _dir) = test_store().await;
        let data = b"some data";
        let wrong_hash = "0000000000000000000000000000000000000000000000000000000000000000";

        let err = store
            .store_chunk("bad-chunk", data, wrong_hash)
            .await
            .expect_err("should reject mismatched hash");
        assert!(matches!(err, RustyStoreError::IntegrityMismatch { .. }));

        // Chunk must not exist on disk after rejected write.
        assert!(!store.chunk_exists("bad-chunk").await.unwrap());
    }

    #[tokio::test]
    async fn test_integrity_mismatch_on_verify() {
        let (store, _dir) = test_store().await;
        let chunk_id = "verify-chunk";
        let data = b"data to verify";
        let hash = FsChunkStore::sha256_hex(data);

        store.store_chunk(chunk_id, data, &hash).await.unwrap();

        // Verify with correct hash passes.
        store
            .verify_chunk(chunk_id, &hash)
            .await
            .expect("verify should pass");

        // Verify with wrong hash fails.
        let err = store
            .verify_chunk(
                chunk_id,
                "ffff000000000000000000000000000000000000000000000000000000000000",
            )
            .await
            .expect_err("verify should fail on mismatch");
        assert!(matches!(err, RustyStoreError::IntegrityMismatch { .. }));
    }

    #[tokio::test]
    async fn test_delete_chunk() {
        let (store, _dir) = test_store().await;
        let chunk_id = "delete-chunk";
        let data = b"delete me";
        let hash = FsChunkStore::sha256_hex(data);

        store.store_chunk(chunk_id, data, &hash).await.unwrap();
        assert!(store.chunk_exists(chunk_id).await.unwrap());

        let deleted = store.delete_chunk(chunk_id).await.unwrap();
        assert!(deleted);
        assert!(!store.chunk_exists(chunk_id).await.unwrap());

        // Deleting again returns false.
        let deleted = store.delete_chunk(chunk_id).await.unwrap();
        assert!(!deleted);
    }

    #[tokio::test]
    async fn test_streaming_reader() {
        let (store, _dir) = test_store().await;
        let chunk_id = "stream-chunk";
        let data = b"streaming read test data";
        let hash = FsChunkStore::sha256_hex(data);

        store.store_chunk(chunk_id, data, &hash).await.unwrap();

        let (meta, mut reader) = store
            .open_chunk_reader(chunk_id)
            .await
            .expect("open reader should succeed");
        assert_eq!(meta.size_bytes, data.len() as u64);

        let mut buf = Vec::new();
        reader.read_to_end(&mut buf).await.unwrap();
        assert_eq!(buf, data);

        // Verify hash separately.
        store.verify_chunk(chunk_id, &hash).await.unwrap();
    }

    #[tokio::test]
    async fn test_get_capacity() {
        let (store, _dir) = test_store().await;
        let cap = store.get_capacity().await.expect("capacity should work");
        let _ = cap;
    }

    #[tokio::test]
    async fn test_list_chunks_skips_temp_files() {
        let (store, _dir) = test_store().await;

        let a = b"chunk a".to_vec();
        let b = b"chunk b payload".to_vec();
        store
            .store_chunk("chunk-a", &a, &FsChunkStore::sha256_hex(&a))
            .await
            .unwrap();
        store
            .store_chunk("chunk-b", &b, &FsChunkStore::sha256_hex(&b))
            .await
            .unwrap();

        // A leftover temp file must never be reported as a stored chunk.
        std::fs::write(store.temp_path("chunk-c"), b"partial").unwrap();

        let listed = store.list_chunks().await.unwrap();
        let ids: Vec<&str> = listed.iter().map(|c| c.chunk_id.as_str()).collect();
        assert_eq!(ids, vec!["chunk-a", "chunk-b"]);
        assert_eq!(listed[0].size_bytes, a.len() as u64);
        assert_eq!(listed[1].size_bytes, b.len() as u64);
        assert!(listed.iter().all(|c| c.modified_epoch_ms > 0));
    }

    #[tokio::test]
    async fn test_atomic_write_no_partial_visible() {
        let (store, _dir) = test_store().await;
        let chunk_id = "atomic-chunk";
        let data = vec![0xAAu8; 1024 * 1024]; // 1 MB
        let hash = FsChunkStore::sha256_hex(&data);

        store.store_chunk(chunk_id, &data, &hash).await.unwrap();

        // Final path must exist, temp path must not.
        assert!(store.chunk_path(chunk_id).metadata().is_ok());
        assert!(!store.temp_path(chunk_id).metadata().is_ok());

        let entry = store.retrieve_chunk(chunk_id).await.unwrap();
        assert_eq!(entry.data, data);
    }
}
