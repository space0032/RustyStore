//! Placement strategy for chunk replicas.
//!
//! Places replicas on distinct healthy nodes with sufficient disk space.

use common::error::RustyStoreError;
use metadata::models::StorageNodeRow;
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
