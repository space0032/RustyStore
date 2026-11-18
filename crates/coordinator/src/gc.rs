//! Orphan-chunk garbage collection and abandoned-upload reaping.
//!
//! Chunk bytes can outlive their metadata: uploads rejected by the size limit,
//! client disconnects, and best-effort deletes all leave files on storage nodes
//! with no `chunks` row pointing at them. The collector lists chunks on every
//! healthy node, subtracts the ids still referenced in PostgreSQL, and deletes
//! the remainder — but only once they are older than a grace period, so chunks
//! belonging to an upload that is still in flight are never touched.

use metadata::MetadataStore;
use proto::storage::{ListChunksRequest, chunk_service_client::ChunkServiceClient};
use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::task::JoinHandle;
use tracing::{info, warn};
use uuid::Uuid;

/// How often to run the collector and how old an unreferenced chunk must be.
#[derive(Clone, Debug)]
pub struct GcConfig {
    pub interval: Duration,
    pub grace: Duration,
    pub upload_ttl: Duration,
}

impl GcConfig {
    /// Read `ORPHAN_GC_INTERVAL_SECS`, `ORPHAN_GRACE_SECS`, and
    /// `UPLOAD_SESSION_TTL_SECS` from the environment.
    ///
    /// An interval of zero disables collection (`Ok(None)`).
    pub fn from_env() -> Result<Option<Self>, common::config::ConfigError> {
        let interval_secs: u64 = common::config::parse_env("ORPHAN_GC_INTERVAL_SECS", 300)?;
        if interval_secs == 0 {
            return Ok(None);
        }

        let grace_secs: u64 = common::config::parse_env("ORPHAN_GRACE_SECS", 3600)?;
        common::config::require_positive("ORPHAN_GRACE_SECS", grace_secs as i64)?;

        let upload_ttl_secs: u64 = common::config::parse_env("UPLOAD_SESSION_TTL_SECS", 86_400)?;
        common::config::require_positive("UPLOAD_SESSION_TTL_SECS", upload_ttl_secs as i64)?;

        Ok(Some(Self {
            interval: Duration::from_secs(interval_secs),
            grace: Duration::from_secs(grace_secs),
            upload_ttl: Duration::from_secs(upload_ttl_secs),
        }))
    }
}

/// A chunk as reported by a storage node's directory listing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListedChunk {
    pub chunk_id: String,
    pub size_bytes: u64,
    pub modified_epoch_ms: i64,
}

/// Counters for one collection pass.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GcStats {
    pub chunks_deleted: u64,
    pub bytes_deleted: u64,
    pub stale_uploads: u64,
}

/// Pure keep/delete decision: listed chunks that are old enough and have no
/// metadata row. Ids that are not UUIDs are skipped (the coordinator only ever
/// creates UUID chunk ids).
pub fn orphan_candidates(
    listed: &[ListedChunk],
    referenced: &HashSet<Uuid>,
    cutoff_epoch_ms: i64,
) -> Vec<(Uuid, u64)> {
    listed
        .iter()
        .filter(|chunk| chunk.modified_epoch_ms <= cutoff_epoch_ms)
        .filter_map(|chunk| {
            Uuid::parse_str(&chunk.chunk_id)
                .ok()
                .map(|id| (id, chunk.size_bytes))
        })
        .filter(|(id, _)| !referenced.contains(id))
        .collect()
}

/// Spawn the periodic collector. Call only when collection is enabled.
pub fn spawn(
    store: Arc<MetadataStore>,
    config: GcConfig,
    is_leader: Arc<AtomicBool>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(config.interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        // The first interval tick resolves immediately; skip it so a restart
        // does not trigger a full cluster sweep before serving traffic.
        ticker.tick().await;

        loop {
            ticker.tick().await;
            if !is_leader.load(Ordering::Relaxed) {
                continue;
            }
            match run_once(&store, config.grace, config.upload_ttl).await {
                Ok(stats) => {
                    if stats != GcStats::default() {
                        info!(
                            "orphan GC: deleted {} chunks ({} bytes), reaped {} stale uploads",
                            stats.chunks_deleted, stats.bytes_deleted, stats.stale_uploads
                        );
                    }
                }
                Err(e) => warn!("orphan GC pass failed: {e}"),
            }
        }
    })
}

/// Run one collection pass over all healthy storage nodes.
pub async fn run_once(
    store: &MetadataStore,
    grace: Duration,
    upload_ttl: Duration,
) -> Result<GcStats, Box<dyn std::error::Error + Send + Sync>> {
    let mut stats = GcStats {
        stale_uploads: store
            .reap_stale_uploads(upload_ttl.as_secs() as i64)
            .await?,
        ..GcStats::default()
    };
    if stats.stale_uploads > 0 {
        observability::STALE_UPLOADS_REAPED.inc_by(stats.stale_uploads);
    }

    let cutoff_epoch_ms = now_epoch_ms() - grace.as_millis() as i64;
    let nodes = store.healthy_nodes().await?;

    for node in nodes {
        let addr = format!("http://{}", node.grpc_addr);
        let mut client = match ChunkServiceClient::connect(addr).await {
            Ok(client) => client,
            Err(e) => {
                warn!("orphan GC: cannot connect to node {}: {e}", node.node_id);
                continue;
            }
        };

        let listed = match list_chunks(&mut client).await {
            Ok(listed) => listed,
            Err(e) => {
                warn!("orphan GC: listing chunks on {} failed: {e}", node.node_id);
                continue;
            }
        };

        // Only ask PostgreSQL about chunks old enough to be collectable.
        let candidate_ids: Vec<Uuid> = listed
            .iter()
            .filter(|chunk| chunk.modified_epoch_ms <= cutoff_epoch_ms)
            .filter_map(|chunk| Uuid::parse_str(&chunk.chunk_id).ok())
            .collect();
        let referenced = referenced_chunk_ids(store, &candidate_ids).await?;

        for (chunk_id, size_bytes) in orphan_candidates(&listed, &referenced, cutoff_epoch_ms) {
            match client
                .delete_chunk(proto::storage::DeleteChunkRequest {
                    chunk_id: chunk_id.to_string(),
                })
                .await
            {
                Ok(resp) => {
                    if resp.into_inner().deleted {
                        stats.chunks_deleted += 1;
                        stats.bytes_deleted += size_bytes;
                        observability::ORPHAN_CHUNKS_DELETED.inc();
                        observability::ORPHAN_BYTES_DELETED.inc_by(size_bytes);
                    }
                }
                Err(e) => warn!(
                    "orphan GC: deleting chunk {chunk_id} from {} failed: {e}",
                    node.node_id
                ),
            }
        }
    }

    Ok(stats)
}

async fn list_chunks(
    client: &mut ChunkServiceClient<tonic::transport::Channel>,
) -> Result<Vec<ListedChunk>, Box<dyn std::error::Error + Send + Sync>> {
    let mut stream = client
        .list_chunks(tonic::Request::new(ListChunksRequest {}))
        .await?
        .into_inner();

    let mut listed = Vec::new();
    while let Some(chunk) = stream.message().await? {
        listed.push(ListedChunk {
            chunk_id: chunk.chunk_id,
            size_bytes: chunk.size_bytes.max(0) as u64,
            modified_epoch_ms: chunk.modified_epoch_ms,
        });
    }
    Ok(listed)
}

/// Look up referenced ids in bounded batches to keep the query small.
async fn referenced_chunk_ids(
    store: &MetadataStore,
    ids: &[Uuid],
) -> Result<HashSet<Uuid>, sqlx::Error> {
    const BATCH: usize = 1000;
    let mut referenced = HashSet::new();
    for batch in ids.chunks(BATCH) {
        referenced.extend(store.existing_chunk_ids(batch).await?);
    }
    Ok(referenced)
}

fn now_epoch_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn listed(id: &str, mtime: i64) -> ListedChunk {
        ListedChunk {
            chunk_id: id.to_string(),
            size_bytes: 42,
            modified_epoch_ms: mtime,
        }
    }

    #[test]
    fn deletes_only_old_unreferenced_chunks() {
        let referenced_id = Uuid::new_v4();
        let orphan_id = Uuid::new_v4();
        let fresh_id = Uuid::new_v4();

        let listed = vec![
            listed(&referenced_id.to_string(), 1_000),
            listed(&orphan_id.to_string(), 1_000),
            listed(&fresh_id.to_string(), 9_000),
        ];
        let referenced = HashSet::from([referenced_id]);

        let candidates = orphan_candidates(&listed, &referenced, 5_000);
        assert_eq!(candidates, vec![(orphan_id, 42)]);
    }

    #[test]
    fn skips_non_uuid_ids() {
        let listed = vec![listed("not-a-uuid", 1_000), listed("also-bad.tmp", 1_000)];
        assert!(orphan_candidates(&listed, &HashSet::new(), 5_000).is_empty());
    }

    #[test]
    fn keeps_chunks_at_exactly_the_cutoff() {
        let id = Uuid::new_v4();
        let listed = vec![listed(&id.to_string(), 5_000)];
        assert_eq!(orphan_candidates(&listed, &HashSet::new(), 5_000).len(), 1);
    }
}
