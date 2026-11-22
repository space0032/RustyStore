//! End-to-end integration tests.
//!
//! These tests exercise the full chunk lifecycle without requiring a running
//! PostgreSQL or Docker:
//!
//! 1. Generate chunk data with SHA-256
//! 2. Place replicas using the coordinator's placement strategy
//! 3. Store chunks via the storage engine (atomic writes)
//! 4. Retrieve and verify SHA-256 integrity
//! 5. Delete chunks

#[cfg(test)]
mod tests {
    use common::error::RustyStoreError;
    use coordinator::place_replicas;
    use metadata::models::StorageNodeRow;
    use sha2::{Digest, Sha256};
    use storage_engine::{ChunkStore, FsChunkStore};
    use tempfile::TempDir;
    use uuid::Uuid;

    fn sha256_hex(data: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(data);
        hex::encode(hasher.finalize())
    }

    fn make_test_node(node_id: Uuid, healthy: bool, available: i64) -> StorageNodeRow {
        StorageNodeRow {
            node_id,
            addr: "127.0.0.1:50051".to_string(),
            grpc_addr: "127.0.0.1:50051".to_string(),
            capacity_bytes: 100_000_000_000,
            available_bytes: available,
            used_bytes: 0,
            lease_expiry: chrono::Utc::now() + chrono::TimeDelta::try_seconds(3600).unwrap(),
            last_seen: chrono::Utc::now(),
            healthy,
            version: Some("1.0.0".to_string()),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }
    }

    #[tokio::test]
    async fn test_full_chunk_lifecycle() {
        // Setup storage engine
        let dir = TempDir::new().unwrap();
        let store = FsChunkStore::new(dir.path());
        store.init().await.unwrap();

        // Create test data (smaller than chunk size for simplicity)
        let data: Vec<u8> = (0..10_000).map(|i| (i % 256) as u8).collect();
        let hash = sha256_hex(&data);
        let chunk_id = "test-chunk-lifecycle";

        // Store chunk
        store
            .store_chunk(chunk_id, &data, &hash)
            .await
            .expect("store should succeed");

        // Verify chunk exists
        assert!(store.chunk_exists(chunk_id).await.unwrap());

        // Retrieve and verify
        let entry = store
            .retrieve_chunk(chunk_id)
            .await
            .expect("retrieve should succeed");
        assert_eq!(entry.data, data);
        assert_eq!(entry.sha256_hex, hash);
        assert_eq!(entry.size_bytes, data.len() as u64);

        // Verify chunk integrity
        store
            .verify_chunk(chunk_id, &hash)
            .await
            .expect("verify should pass");

        // Delete chunk
        assert!(store.delete_chunk(chunk_id).await.unwrap());
        assert!(!store.chunk_exists(chunk_id).await.unwrap());

        // Deleting again returns false
        assert!(!store.delete_chunk(chunk_id).await.unwrap());
    }

    #[tokio::test]
    async fn test_placement_with_healthy_nodes() {
        let nodes: Vec<StorageNodeRow> = (0..5)
            .map(|_| make_test_node(Uuid::new_v4(), true, 1_000_000_000))
            .collect();

        let result = place_replicas(&nodes, 100, 3, &[]);
        assert!(result.is_ok());
        let chosen = result.unwrap();
        assert_eq!(chosen.len(), 3);

        // All chosen nodes should be distinct
        let unique: std::collections::HashSet<_> = chosen.iter().collect();
        assert_eq!(unique.len(), 3);

        // All chosen nodes should be from our set
        let node_ids: std::collections::HashSet<_> = nodes.iter().map(|n| n.node_id).collect();
        for id in &chosen {
            assert!(node_ids.contains(id));
        }
    }

    #[tokio::test]
    async fn test_placement_excludes_unhealthy_and_insufficient_disk() {
        let nodes: Vec<StorageNodeRow> = vec![
            make_test_node(Uuid::new_v4(), true, 500_000_000), // healthy, enough disk
            make_test_node(Uuid::new_v4(), false, 500_000_000), // unhealthy
            make_test_node(Uuid::new_v4(), true, 10_000_000),  // healthy, not enough disk
            make_test_node(Uuid::new_v4(), true, 500_000_000), // healthy, enough disk
            make_test_node(Uuid::new_v4(), true, 500_000_000), // healthy, enough disk
        ];

        // Need 3 replicas with 100MB each
        // Only nodes 0, 3, 4 qualify (healthy + enough disk)
        let result = place_replicas(&nodes, 100_000_000, 3, &[]);
        assert!(result.is_ok());
        let chosen = result.unwrap();
        assert_eq!(chosen.len(), 3);

        // Node 1 (unhealthy) and node 2 (insufficient disk) should not be chosen
        assert!(!chosen.contains(&nodes[1].node_id));
        assert!(!chosen.contains(&nodes[2].node_id));
    }

    #[tokio::test]
    async fn test_duplicate_chunk_dedup_by_hash() {
        let dir = TempDir::new().unwrap();
        let store = FsChunkStore::new(dir.path());
        store.init().await.unwrap();

        // Store same data under two different chunk IDs
        let data: Vec<u8> = vec![0xAB; 1024];
        let hash = sha256_hex(&data);

        store.store_chunk("chunk-a", &data, &hash).await.unwrap();
        store.store_chunk("chunk-b", &data, &hash).await.unwrap();

        // Both should exist with the same hash
        assert!(store.chunk_exists("chunk-a").await.unwrap());
        assert!(store.chunk_exists("chunk-b").await.unwrap());

        let entry_a = store.retrieve_chunk("chunk-a").await.unwrap();
        let entry_b = store.retrieve_chunk("chunk-b").await.unwrap();
        assert_eq!(entry_a.sha256_hex, entry_b.sha256_hex);
        assert_eq!(entry_a.data, entry_b.data);

        // Clean up
        store.delete_chunk("chunk-a").await.unwrap();
        store.delete_chunk("chunk-b").await.unwrap();
    }

    #[tokio::test]
    async fn test_corrupted_chunk_detection() {
        let dir = TempDir::new().unwrap();
        let store = FsChunkStore::new(dir.path());
        store.init().await.unwrap();

        let data: Vec<u8> = vec![0x42; 1000];
        let hash = sha256_hex(&data);

        // Store correct data
        store
            .store_chunk("corrupt-test", &data, &hash)
            .await
            .unwrap();

        // Verify with wrong hash should fail
        let err = store
            .verify_chunk(
                "corrupt-test",
                "0000000000000000000000000000000000000000000000000000000000000000",
            )
            .await
            .expect_err("verify should fail on hash mismatch");
        assert!(matches!(err, RustyStoreError::IntegrityMismatch { .. }));

        // Retrieve should still return correct data with correct hash
        let entry = store.retrieve_chunk("corrupt-test").await.unwrap();
        assert_eq!(entry.data, data);

        // Clean up
        store.delete_chunk("corrupt-test").await.unwrap();
    }
}
