//! S3/MinIO integration tests.
//!
//! Enable with `cargo test -p storage-engine --features s3` and point
//! `TEST_S3_ENDPOINT` at a disposable S3-compatible service (MinIO works).
//! Optional: `TEST_S3_BUCKET` (default `rustystore-test`), `TEST_S3_ACCESS_KEY`
//! and `TEST_S3_SECRET_KEY` (default `minioadmin`), `TEST_S3_REGION`.

#![cfg(feature = "s3")]

use common::error::RustyStoreError;
use sha2::{Digest, Sha256};
use storage_engine::{ChunkStore, S3ChunkStore};

fn sha256_hex(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hex::encode(hasher.finalize())
}

async fn store() -> Option<S3ChunkStore> {
    let endpoint = std::env::var("TEST_S3_ENDPOINT").ok()?;
    let bucket = std::env::var("TEST_S3_BUCKET").unwrap_or_else(|_| "rustystore-test".to_string());
    let access = std::env::var("TEST_S3_ACCESS_KEY").unwrap_or_else(|_| "minioadmin".to_string());
    let secret = std::env::var("TEST_S3_SECRET_KEY").unwrap_or_else(|_| "minioadmin".to_string());
    let region = std::env::var("TEST_S3_REGION").unwrap_or_else(|_| "us-east-1".to_string());
    let prefix = format!(
        "integration-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0)
    );

    let store = S3ChunkStore::new(
        &endpoint,
        &bucket,
        &prefix,
        Some(&access),
        Some(&secret),
        Some(&region),
    )
    .await
    .expect("create S3 store");
    store.ensure_bucket().await.expect("ensure bucket");
    Some(store)
}

#[tokio::test]
async fn s3_store_roundtrip_and_integrity() {
    let Some(store) = store().await else {
        return;
    };

    let chunk_id = "chunk-roundtrip";
    let data = b"hello from an S3-compatible backend";
    let digest = sha256_hex(data);

    store
        .store_chunk(chunk_id, data, &digest)
        .await
        .expect("store chunk");
    assert!(store.chunk_exists(chunk_id).await.expect("exists"));

    let entry = store
        .retrieve_chunk(chunk_id)
        .await
        .expect("retrieve chunk");
    assert_eq!(entry.chunk_id, chunk_id);
    assert_eq!(entry.sha256_hex, digest);
    assert_eq!(entry.size_bytes, data.len() as u64);
    assert_eq!(entry.data, data);

    store
        .verify_chunk(chunk_id, &digest)
        .await
        .expect("verify chunk");

    let listed = store.list_chunks().await.expect("list chunks");
    assert!(
        listed.iter().any(|c| c.chunk_id == chunk_id),
        "stored chunk should be listed"
    );

    assert!(store.delete_chunk(chunk_id).await.expect("delete chunk"));
    assert!(!store.chunk_exists(chunk_id).await.expect("exists"));
    assert!(
        !store
            .delete_chunk(chunk_id)
            .await
            .expect("delete missing chunk")
    );
    assert!(matches!(
        store.retrieve_chunk(chunk_id).await,
        Err(RustyStoreError::NotFound(_))
    ));
}

#[tokio::test]
async fn s3_store_rejects_corrupted_uploads() {
    let Some(store) = store().await else {
        return;
    };

    let chunk_id = "chunk-corrupt";
    let data = b"payload";
    let wrong = sha256_hex(b"something else");

    assert!(matches!(
        store.store_chunk(chunk_id, data, &wrong).await,
        Err(RustyStoreError::IntegrityMismatch { .. })
    ));
    assert!(!store.chunk_exists(chunk_id).await.expect("exists"));
}
