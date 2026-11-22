//! Placement strategy for chunk replicas.
//!
//! Places replicas on distinct healthy nodes with sufficient disk space.

use common::error::RustyStoreError;
use metadata::models::StorageNodeRow;
#[allow(unused_imports)]
use std::collections::HashSet;
use uuid::Uuid;

pub type PlacementResult = Result<Vec<Uuid>, RustyStoreError>;

/// Select `replication_factor` distinct healthy nodes with enough free space.
pub fn place_replicas(
    nodes: &[StorageNodeRow],
    needed_bytes: i64,
    replication_factor: i32,
    exclude: &[Uuid],
) -> PlacementResult {
    let mut eligible: Vec<&StorageNodeRow> = nodes
        .iter()
        .filter(|n| n.healthy && n.available_bytes >= needed_bytes && !exclude.contains(&n.node_id))
        .collect();

    if eligible.len() < replication_factor as usize {
        return Err(RustyStoreError::InsufficientNodes {
            required: replication_factor,
            available: eligible.len() as i32,
        });
    }

    eligible.sort_by_key(|n| n.node_id);

    let chosen: Vec<Uuid> = eligible
        .into_iter()
        .take(replication_factor as usize)
        .map(|n| n.node_id)
        .collect();
    Ok(chosen)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use metadata::models::StorageNodeRow;

    fn make_node(node_id: Uuid, healthy: bool, available: i64) -> StorageNodeRow {
        StorageNodeRow {
            node_id,
            addr: "127.0.0.1:50051".to_string(),
            grpc_addr: "127.0.0.1:50051".to_string(),
            capacity_bytes: 100_000_000_000,
            available_bytes: available,
            used_bytes: 0,
            lease_expiry: Utc::now() + chrono::TimeDelta::try_seconds(3600).unwrap(),
            last_seen: Utc::now(),
            healthy,
            version: Some("1.0.0".to_string()),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn test_place_replicas_selects_distinct_nodes() {
        let nodes: Vec<StorageNodeRow> = (0..5)
            .map(|_| make_node(Uuid::new_v4(), true, 1_000_000_000))
            .collect();

        let result = place_replicas(&nodes, 100, 3, &[]);
        assert!(result.is_ok());
        let chosen = result.unwrap();
        assert_eq!(chosen.len(), 3);
        // All distinct
        assert_eq!(chosen.iter().collect::<HashSet<_>>().len(), 3);
    }

    #[test]
    fn test_place_replicas_insufficient_nodes() {
        let nodes: Vec<StorageNodeRow> = vec![make_node(Uuid::new_v4(), true, 1_000_000_000)];

        let result = place_replicas(&nodes, 100, 3, &[]);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            RustyStoreError::InsufficientNodes {
                required: 3,
                available: 1
            }
        ));
    }

    #[test]
    fn test_place_replicas_excludes_unhealthy_nodes() {
        let nodes: Vec<StorageNodeRow> = vec![
            make_node(Uuid::new_v4(), true, 1_000_000_000),
            make_node(Uuid::new_v4(), false, 1_000_000_000), // unhealthy
            make_node(Uuid::new_v4(), true, 1_000_000_000),
            make_node(Uuid::new_v4(), true, 1_000_000_000),
        ];

        let result = place_replicas(&nodes, 100, 3, &[]);
        assert!(result.is_ok());
        let chosen = result.unwrap();
        assert_eq!(chosen.len(), 3);
        // None of the chosen should be the unhealthy node
        let unhealthy_id = nodes[1].node_id;
        assert!(!chosen.contains(&unhealthy_id));
    }

    #[test]
    fn test_place_replicas_respects_disk_space() {
        let nodes: Vec<StorageNodeRow> = vec![
            make_node(Uuid::new_v4(), true, 500_000_000), // enough
            make_node(Uuid::new_v4(), true, 10_000_000),  // not enough for 100MB
            make_node(Uuid::new_v4(), true, 500_000_000), // enough
        ];

        let result = place_replicas(&nodes, 100_000_000, 2, &[]);
        assert!(result.is_ok());
        let chosen = result.unwrap();
        assert_eq!(chosen.len(), 2);
        // Both chosen nodes should be the ones with enough disk
        let insufficient_id = nodes[1].node_id;
        assert!(!chosen.contains(&insufficient_id));
    }

    #[test]
    fn test_place_replicas_excludes_specified_nodes() {
        let nodes: Vec<StorageNodeRow> = (0..4)
            .map(|_| make_node(Uuid::new_v4(), true, 1_000_000_000))
            .collect();

        let exclude = vec![nodes[0].node_id, nodes[1].node_id];
        let result = place_replicas(&nodes, 100, 2, &exclude);
        assert!(result.is_ok());
        let chosen = result.unwrap();
        assert_eq!(chosen.len(), 2);
        assert!(!chosen.contains(&nodes[0].node_id));
        assert!(!chosen.contains(&nodes[1].node_id));
    }
}
