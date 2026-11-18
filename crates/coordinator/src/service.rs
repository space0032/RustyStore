//! Coordinator gRPC service implementation + background tasks.
//!
//! Implements NodeService (Register, Heartbeat, ReportCapacity, Check) and
//! ClusterService (gateway-facing RPCs). Runs background tasks for lease
//! expiry monitoring, under-replication detection, and recovery scheduling.

use crate::gc::{self, GcConfig};
use crate::placement::place_replicas;
#[allow(unused_imports)]
use crate::types::{ChunkLocationEntry, ChunkManifestEntry, ClusterStats, Manifest, NodeInfoEntry};
use futures_util::StreamExt;
use futures_util::stream::{FuturesUnordered, Stream};
use metadata::models::{ChunkReplicaRow, ChunkRow, StorageNodeRow};
use metadata::{MetadataStore, ReplicationTaskRow};
use proto::storage::{
    CapacityReport, ChunkLocation, ChunkReference, CompleteUploadRequest, CompleteUploadResponse,
    CreateUploadSessionRequest, CreateUploadSessionResponse, DeleteFileRequest, DeleteFileResponse,
    Empty, FileSummary, GetChunkPlanRequest, GetChunkPlanResponse, GetManifestRequest,
    GetManifestResponse, GetStatsRequest, GetStatsResponse, HealthCheckRequest,
    HealthCheckResponse, HeartbeatRequest, HeartbeatResponse, ListFilesRequest, ListFilesResponse,
    ListNodesRequest, ListNodesResponse, NodeInfo, RegisterRequest, RegisterResponse,
    RetrieveChunkRequest, StoreChunkRequest,
    chunk_service_client::ChunkServiceClient,
    cluster_service_server::{ClusterService, ClusterServiceServer},
    node_service_server::NodeService,
};
#[allow(unused_imports)]
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::task::JoinHandle;
use tonic::{Request, Response, Status, Streaming};
use tracing::{info, warn};
use uuid::Uuid;

type BoxStream<T> = Pin<Box<dyn Stream<Item = Result<T, Status>> + Send>>;

const DEFAULT_LIST_LIMIT: i32 = 50;
const MAX_LIST_LIMIT: i32 = 200;

/// How often a coordinator renews its leadership lease.
const LEADERSHIP_RENEW_INTERVAL: Duration = Duration::from_secs(5);
/// Lease duration; a crashed coordinator's background work stops within this
/// window after the process dies.
const LEADERSHIP_LEASE_MS: i64 = 15_000;
/// An `in_progress` recovery task untouched for this long is considered
/// abandoned and is failed so a fresh task can be scheduled.
const STALE_REPLICATION_TASK_SECS: i64 = 300;

/// Delay needed to transfer `bytes` at the configured rate, if any.
fn throttle_delay(bandwidth_bytes_per_sec: Option<u64>, bytes: usize) -> Option<Duration> {
    let rate = bandwidth_bytes_per_sec.filter(|rate| *rate > 0)?;
    if bytes == 0 {
        return None;
    }
    Some(Duration::from_secs_f64(bytes as f64 / rate as f64))
}

async fn throttle(bandwidth_bytes_per_sec: Option<u64>, bytes: usize) {
    if let Some(delay) = throttle_delay(bandwidth_bytes_per_sec, bytes) {
        tokio::time::sleep(delay).await;
    }
}

/// Opaque keyset cursor: `<created_at_micros>:<file_id>`.
fn encode_file_cursor(created_at: chrono::DateTime<chrono::Utc>, file_id: Uuid) -> String {
    format!("{}:{}", created_at.timestamp_micros(), file_id)
}

#[allow(clippy::result_large_err)] // tonic::Status is the natural gRPC error here
fn decode_file_cursor(cursor: &str) -> Result<(i64, Uuid), Status> {
    let (micros, file_id) = cursor
        .split_once(':')
        .ok_or_else(|| Status::invalid_argument("invalid cursor"))?;
    let micros = micros
        .parse::<i64>()
        .map_err(|_| Status::invalid_argument("invalid cursor"))?;
    let file_id =
        Uuid::parse_str(file_id).map_err(|_| Status::invalid_argument("invalid cursor"))?;
    Ok((micros, file_id))
}

/// Build a gRPC endpoint from a node address, tolerating addresses that
/// already carry an `http(s)://` scheme.
fn node_endpoint(addr: &str) -> String {
    if addr.starts_with("http://") || addr.starts_with("https://") {
        addr.to_string()
    } else {
        format!("http://{addr}")
    }
}

/// Order the replicas to try as recovery sources.
///
/// The task's recorded source is preferred, then any other healthy durable
/// replica. The target is excluded (it is the destination) and duplicates are
/// removed so a source is only attempted once.
fn source_candidates(
    preferred: Option<Uuid>,
    replicas: &[ChunkReplicaRow],
    healthy: &std::collections::HashSet<Uuid>,
    target: Uuid,
) -> Vec<Uuid> {
    let mut candidates = Vec::new();
    let mut consider = |node_id: Uuid| {
        if node_id != target && healthy.contains(&node_id) && !candidates.contains(&node_id) {
            candidates.push(node_id);
        }
    };
    if let Some(node_id) = preferred {
        consider(node_id);
    }
    for replica in replicas.iter().filter(|r| r.durable) {
        consider(replica.node_id);
    }
    candidates
}

/// Order the nodes to try as replication targets.
///
/// The task's recorded target is preferred, then the other healthy nodes. The
/// source and any node that already has a replica row (durable or not) are
/// excluded, so a target is only attempted once.
fn target_candidates(
    preferred: Uuid,
    healthy: &[Uuid],
    replicas: &[ChunkReplicaRow],
    source: Uuid,
) -> Vec<Uuid> {
    let healthy_set: std::collections::HashSet<Uuid> = healthy.iter().copied().collect();
    let occupied: std::collections::HashSet<Uuid> = replicas.iter().map(|r| r.node_id).collect();
    let mut candidates = Vec::new();
    let mut consider = |node_id: Uuid| {
        if node_id != source
            && healthy_set.contains(&node_id)
            && !occupied.contains(&node_id)
            && !candidates.contains(&node_id)
        {
            candidates.push(node_id);
        }
    };
    consider(preferred);
    for node_id in healthy {
        consider(*node_id);
    }
    candidates
}

/// The Coordinator: gRPC service + background recovery tasks.
pub struct Coordinator {
    service: CoordinatorService,
    #[allow(dead_code)]
    leadership_handle: Option<JoinHandle<()>>,
    #[allow(dead_code)]
    lease_monitor_handle: Option<JoinHandle<()>>,
    #[allow(dead_code)]
    replication_monitor_handle: Option<JoinHandle<()>>,
    #[allow(dead_code)]
    recovery_scheduler_handle: Option<JoinHandle<()>>,
    #[allow(dead_code)]
    orphan_gc_handle: Option<JoinHandle<()>>,
}

impl Coordinator {
    pub fn new(
        store: Arc<MetadataStore>,
        replication_factor: i32,
        chunk_size_bytes: i64,
        gc_config: Option<GcConfig>,
        recovery_bandwidth_bytes_per_sec: Option<u64>,
    ) -> Self {
        let service = CoordinatorService::new(store.clone(), replication_factor, chunk_size_bytes);
        let is_leader = Arc::new(AtomicBool::new(false));

        // Leadership: only the coordinator holding the database lease runs
        // background work. Every coordinator can serve gateway RPCs, so the
        // gateway can fail over to a standby without waiting for a lease.
        let leadership = {
            let store = store.clone();
            let is_leader = is_leader.clone();
            let leader_id = Uuid::new_v4();
            tokio::spawn(async move {
                let mut interval = tokio::time::interval(LEADERSHIP_RENEW_INTERVAL);
                let mut was_leader = false;
                loop {
                    interval.tick().await;
                    match store
                        .try_acquire_leadership(leader_id, LEADERSHIP_LEASE_MS)
                        .await
                    {
                        Ok(leader) => {
                            is_leader.store(leader, Ordering::Relaxed);
                            observability::COORDINATOR_IS_LEADER.set(i64::from(leader));
                            if leader != was_leader {
                                if leader {
                                    info!("coordinator {leader_id} acquired leadership");
                                } else {
                                    info!("coordinator {leader_id} is a standby");
                                }
                                was_leader = leader;
                            }
                        }
                        Err(e) => {
                            is_leader.store(false, Ordering::Relaxed);
                            observability::COORDINATOR_IS_LEADER.set(0);
                            if was_leader {
                                warn!("lost coordinator leadership: {e}");
                                was_leader = false;
                            }
                        }
                    }
                }
            })
        };

        // Lease expiry monitor: marks nodes unhealthy when lease expires.
        let lease_monitor = {
            let store = store.clone();
            let is_leader = is_leader.clone();
            let interval = Duration::from_secs(5);
            tokio::spawn(async move {
                let mut interval = tokio::time::interval(interval);
                loop {
                    interval.tick().await;
                    if is_leader.load(Ordering::Relaxed) {
                        if let Err(e) = store.expire_leases().await {
                            warn!("lease expiry monitor error: {e}");
                        }
                        match store
                            .fail_stale_replication_tasks(STALE_REPLICATION_TASK_SECS)
                            .await
                        {
                            Ok(count) if count > 0 => {
                                warn!("marked {count} abandoned replication tasks as failed");
                            }
                            Ok(_) => {}
                            Err(e) => warn!("stale replication task cleanup error: {e}"),
                        }
                    }
                    if let Ok(nodes) = store.healthy_nodes().await {
                        observability::ACTIVE_NODES.set(nodes.len() as i64);
                    }
                }
            })
        };

        // Under-replication detector: finds under-replicated chunks and
        // creates replication tasks.
        let replication_monitor = {
            let store = store.clone();
            let is_leader = is_leader.clone();
            let rf = replication_factor;
            let interval = Duration::from_secs(10);
            tokio::spawn(async move {
                let mut interval = tokio::time::interval(interval);
                loop {
                    interval.tick().await;
                    if !is_leader.load(Ordering::Relaxed) {
                        continue;
                    }
                    if let Err(e) = Self::detect_under_replicated(&store, rf).await {
                        warn!("under-replication detector error: {e}");
                    }
                }
            })
        };

        // Recovery scheduler: claims replication tasks and executes them
        // with bounded concurrency (max 4 concurrent recoveries).
        let recovery_scheduler = {
            let store = store.clone();
            let is_leader = is_leader.clone();
            let interval = Duration::from_secs(5);
            let max_concurrent = 4usize;
            tokio::spawn(async move {
                let mut interval = tokio::time::interval(interval);
                let semaphore = Arc::new(tokio::sync::Semaphore::new(max_concurrent));
                loop {
                    interval.tick().await;
                    if !is_leader.load(Ordering::Relaxed) {
                        continue;
                    }
                    let store = store.clone();
                    let semaphore = semaphore.clone();
                    tokio::spawn(async move {
                        if let Err(e) = Self::run_recovery_batch(
                            &store,
                            semaphore,
                            recovery_bandwidth_bytes_per_sec,
                        )
                        .await
                        {
                            warn!("recovery scheduler batch error: {e}");
                        }
                    });
                }
            })
        };

        // Orphan-chunk collector: reclaims bytes with no metadata row and
        // reaps uploads that never completed. Disabled when the interval is 0.
        let orphan_gc = gc_config.map(|config| gc::spawn(store.clone(), config, is_leader));

        Self {
            service,
            leadership_handle: Some(leadership),
            lease_monitor_handle: Some(lease_monitor),
            replication_monitor_handle: Some(replication_monitor),
            recovery_scheduler_handle: Some(recovery_scheduler),
            orphan_gc_handle: orphan_gc,
        }
    }

    /// Detect under-replicated chunks and create replication tasks.
    async fn detect_under_replicated(
        store: &Arc<MetadataStore>,
        _default_rf: i32,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let under_replicated = store.under_replicated_chunks().await?;
        observability::UNDER_REPLICATED_CHUNKS.set(under_replicated.len() as i64);

        for (chunk_id, _file_id, required, actual) in under_replicated {
            // One active task per chunk: the detector ticks every 10s, and
            // without this check a slow or failing replication would enqueue
            // another task each tick.
            if store.has_active_replication_task(chunk_id).await? {
                continue;
            }

            // Find a healthy source node that has a durable replica of this chunk.
            let replicas = store.replicas_for_chunk(chunk_id).await?;
            let healthy_nodes = store.healthy_nodes().await?;

            let source_node_id = replicas.iter().filter(|r| r.durable).find_map(|r| {
                healthy_nodes
                    .iter()
                    .find(|n| n.node_id == r.node_id)
                    .map(|n| n.node_id)
            });

            let Some(source_node_id) = source_node_id else {
                warn!("chunk {chunk_id} has no healthy source replica");
                continue;
            };

            // Find healthy target nodes that don't already have this chunk.
            let existing_nodes: Vec<Uuid> = replicas.iter().map(|r| r.node_id).collect();
            let target_candidates: Vec<StorageNodeRow> = healthy_nodes
                .iter()
                .filter(|n| !existing_nodes.contains(&n.node_id))
                .cloned()
                .collect();

            if target_candidates.is_empty() {
                warn!("chunk {chunk_id} has no healthy target node for replication");
                continue;
            }

            // Use placement strategy to pick a target.
            let targets = place_replicas(&target_candidates, 0, 1, &existing_nodes)?;

            if let Some(target_node_id) = targets.first() {
                let task_id = Uuid::new_v4();
                let priority = (i64::from(required) - actual).clamp(1, i64::from(i32::MAX)) as i32;
                store
                    .create_replication_task(
                        task_id,
                        chunk_id,
                        Some(source_node_id),
                        *target_node_id,
                        5,
                        priority,
                    )
                    .await?;
                info!(
                    "created replication task for chunk {chunk_id}: node {source_node_id} -> {} (priority {priority})",
                    target_node_id
                );
            }
        }

        Ok(())
    }

    /// Run one recovery batch: claim multiple pending tasks and process them
    /// concurrently with bounded semaphore.
    async fn run_recovery_batch(
        store: &Arc<MetadataStore>,
        semaphore: Arc<tokio::sync::Semaphore>,
        bandwidth_bytes_per_sec: Option<u64>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Claim up to 8 pending tasks.
        let mut tasks = Vec::new();
        for _ in 0..8 {
            match store.claim_replication_task().await? {
                Some(task) => tasks.push(task),
                None => break,
            }
        }

        if tasks.is_empty() {
            return Ok(());
        }

        // Process all claimed tasks concurrently with bounded semaphore.
        let mut join_set = tokio::task::JoinSet::new();
        for task in tasks {
            let store = store.clone();
            let permit = semaphore.clone().acquire_owned().await?;
            join_set.spawn(async move {
                let _permit = permit;
                if let Err(e) =
                    Self::execute_recovery_task(&store, task, bandwidth_bytes_per_sec).await
                {
                    warn!("recovery task error: {e}");
                }
            });
        }

        // Wait for all tasks to complete.
        while join_set.join_next().await.is_some() {}

        Ok(())
    }

    /// Execute a single replication task: transfer chunk from a source replica
    /// to a target. If the recorded source is unhealthy or fails mid-transfer,
    /// try the chunk's other healthy durable replicas. If the recorded target
    /// is unhealthy or fails, try another healthy node that does not already
    /// hold the chunk.
    async fn execute_recovery_task(
        store: &Arc<MetadataStore>,
        task: ReplicationTaskRow,
        bandwidth_bytes_per_sec: Option<u64>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let chunk_id = task.chunk_id;
        let preferred_target = task.target_node_id;
        let preferred_source = task.source_node_id;

        // Look up node addresses.
        let nodes = store.all_nodes().await?;
        let node_map: std::collections::HashMap<Uuid, StorageNodeRow> =
            nodes.into_iter().map(|n| (n.node_id, n)).collect();

        // Get chunk metadata (SHA-256, size) from the chunks table.
        let Some(chunk_row) = store.get_chunk(chunk_id).await? else {
            warn!(
                "chunk {chunk_id} not found in metadata, failing task {}",
                task.task_id
            );
            store
                .mark_task_failed(task.task_id, task.attempts + 1)
                .await?;
            return Ok(());
        };

        // The recorded source may have died since the task was created. Build
        // an ordered candidate list: the recorded source first (if still
        // healthy), then every other healthy durable replica of the chunk.
        let replicas = store.replicas_for_chunk(chunk_id).await?;
        let healthy_nodes = store.healthy_nodes().await?;
        let healthy_ids: Vec<Uuid> = healthy_nodes.iter().map(|n| n.node_id).collect();
        let healthy: std::collections::HashSet<Uuid> = healthy_ids.iter().copied().collect();
        let sources = source_candidates(preferred_source, &replicas, &healthy, preferred_target);

        let mut source_and_data = None;
        let mut fetches = FuturesUnordered::new();
        for source_node_id in sources {
            let Some(source_node) = node_map.get(&source_node_id).cloned() else {
                warn!("source node {source_node_id} has a replica but no node row; skipping");
                continue;
            };
            let expected_sha256 = chunk_row.sha256_hex.clone();
            fetches.push(async move {
                let result = Self::retrieve_verified_chunk(
                    &source_node,
                    chunk_id,
                    &expected_sha256,
                    bandwidth_bytes_per_sec,
                )
                .await;
                (source_node_id, result)
            });
        }

        // Race the candidate sources and use the first verified response. The
        // remaining in-flight transfers are cancelled when `fetches` is
        // dropped, so a slow or corrupt replica cannot delay recovery.
        while let Some((source_node_id, result)) = fetches.next().await {
            match result {
                Ok(data) => {
                    if preferred_source != Some(source_node_id) {
                        info!(
                            "recovery task {} using alternate source {source_node_id}",
                            task.task_id
                        );
                    }
                    source_and_data = Some((source_node_id, data));
                    break;
                }
                Err(e) => warn!(
                    "source node {source_node_id} failed for chunk {chunk_id}: {e}; waiting for another replica"
                ),
            }
        }

        let Some((source_node_id, chunk_data)) = source_and_data else {
            warn!(
                "no usable source replica for chunk {chunk_id}, failing task {}",
                task.task_id
            );
            store
                .mark_task_failed(task.task_id, task.attempts + 1)
                .await?;
            return Ok(());
        };

        // The recorded target may also have died or lost its lease. Try it
        // first, then any other healthy node that does not already hold the
        // chunk. Nodes with even a non-durable replica row are skipped.
        let targets = target_candidates(preferred_target, &healthy_ids, &replicas, source_node_id);
        if targets.is_empty() {
            warn!(
                "chunk {chunk_id} has no healthy target node for replication, failing task {}",
                task.task_id
            );
            store
                .mark_task_failed(task.task_id, task.attempts + 1)
                .await?;
            return Ok(());
        }

        let mut stored_on = None;
        for target_node_id in targets {
            let Some(target_node) = node_map.get(&target_node_id) else {
                warn!("target node {target_node_id} has no node row; skipping");
                continue;
            };
            match Self::push_chunk_to_target(
                target_node,
                &chunk_row,
                &chunk_data,
                bandwidth_bytes_per_sec,
            )
            .await
            {
                Ok(()) => {
                    if target_node_id != preferred_target {
                        info!(
                            "recovery task {} using alternate target {target_node_id}",
                            task.task_id
                        );
                    }
                    stored_on = Some(target_node_id);
                    break;
                }
                Err(e) => warn!(
                    "target node {target_node_id} failed for chunk {chunk_id}: {e}; trying another node"
                ),
            }
        }

        let Some(target_node_id) = stored_on else {
            warn!(
                "no usable target node for chunk {chunk_id}, failing task {}",
                task.task_id
            );
            store
                .mark_task_failed(task.task_id, task.attempts + 1)
                .await?;
            return Ok(());
        };

        // Mark replica as durable in metadata.
        store.insert_replica(chunk_id, target_node_id, true).await?;
        store.mark_task_done(task.task_id).await?;

        info!(
            "recovery task {} complete: chunk {} -> node {} (durable confirmed)",
            task.task_id, chunk_id, target_node_id
        );

        Ok(())
    }

    /// Push verified chunk bytes to one target and require a durable ack.
    async fn push_chunk_to_target(
        target_node: &StorageNodeRow,
        chunk_row: &ChunkRow,
        chunk_data: &[u8],
        bandwidth_bytes_per_sec: Option<u64>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let target_addr = node_endpoint(&target_node.grpc_addr);
        let mut target_client = ChunkServiceClient::connect(target_addr).await?;

        let meta = StoreChunkRequest {
            payload: Some(proto::storage::store_chunk_request::Payload::Meta(
                ChunkReference {
                    chunk_id: chunk_row.chunk_id.to_string(),
                    file_id: String::new(),
                    chunk_index: 0,
                    size_bytes: chunk_row.size_bytes,
                    sha256_hex: chunk_row.sha256_hex.clone(),
                    replication_factor: 0,
                },
            )),
        };

        let data = chunk_data.to_vec();
        let frame_size = 64 * 1024;
        let store_stream = async_stream::stream! {
            yield meta;
            for chunk in data.chunks(frame_size) {
                throttle(bandwidth_bytes_per_sec, chunk.len()).await;
                yield StoreChunkRequest {
                    payload: Some(proto::storage::store_chunk_request::Payload::Data(chunk.to_vec())),
                };
            }
        };

        let store_response = target_client
            .store_chunk(tonic::Request::new(store_stream))
            .await?
            .into_inner();

        if !store_response.durable {
            return Err(format!(
                "target node {} did not confirm durable storage",
                target_node.node_id
            )
            .into());
        }
        Ok(())
    }

    /// Fetch a chunk from one source replica and verify its SHA-256 before
    /// using it. Returns an error so the caller can try the next replica.
    async fn retrieve_verified_chunk(
        source_node: &StorageNodeRow,
        chunk_id: Uuid,
        expected_sha256: &str,
        bandwidth_bytes_per_sec: Option<u64>,
    ) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
        let source_addr = node_endpoint(&source_node.grpc_addr);
        let mut source_client = ChunkServiceClient::connect(source_addr).await?;

        let retrieve_req = RetrieveChunkRequest {
            chunk_id: chunk_id.to_string(),
            expected_sha256: expected_sha256.to_string(),
        };
        let mut retrieve_stream = source_client
            .retrieve_chunk(tonic::Request::new(retrieve_req))
            .await?
            .into_inner();

        let mut chunk_data = Vec::new();
        while let Some(frame) = retrieve_stream.message().await? {
            match frame.payload {
                Some(proto::storage::retrieve_chunk_response::Payload::Meta(_)) => {}
                Some(proto::storage::retrieve_chunk_response::Payload::Data(data)) => {
                    throttle(bandwidth_bytes_per_sec, data.len()).await;
                    chunk_data.extend_from_slice(&data);
                }
                None => {}
            }
        }

        let actual_hash: String = {
            use sha2::{Digest, Sha256};
            let mut hasher = Sha256::new();
            hasher.update(&chunk_data);
            hex::encode(hasher.finalize())
        };
        if actual_hash != expected_sha256 {
            return Err(
                format!("hash mismatch: expected {expected_sha256} got {actual_hash}").into(),
            );
        }
        Ok(chunk_data)
    }

    pub fn service(&self) -> CoordinatorService {
        self.service.clone()
    }

    pub fn into_server(self) -> ClusterServiceServer<CoordinatorService> {
        self.service.into_server()
    }
}

#[derive(Clone)]
pub struct CoordinatorService {
    store: Arc<MetadataStore>,
    replication_factor: i32,
    chunk_size_bytes: i64,
}

impl CoordinatorService {
    pub fn new(store: Arc<MetadataStore>, replication_factor: i32, chunk_size_bytes: i64) -> Self {
        Self {
            store,
            replication_factor,
            chunk_size_bytes,
        }
    }

    pub fn into_server(self) -> ClusterServiceServer<Self> {
        ClusterServiceServer::new(self)
    }
}

#[tonic::async_trait]
impl NodeService for CoordinatorService {
    type HeartbeatStream = BoxStream<HeartbeatResponse>;

    async fn register(
        &self,
        request: Request<Streaming<RegisterRequest>>,
    ) -> Result<Response<RegisterResponse>, Status> {
        let mut stream = request.into_inner();

        // Collect the registration message.
        let reg = stream
            .message()
            .await
            .map_err(|e| Status::internal(e.to_string()))?
            .ok_or_else(|| Status::invalid_argument("empty register stream"))?;

        let node_id = Uuid::new_v4();
        let lease_ttl_ms = 15_000i64;
        let lease_expiry = chrono::Utc::now() + chrono::TimeDelta::try_seconds(15).unwrap();

        self.store
            .upsert_node(
                node_id,
                &reg.addr,
                &reg.grpc_addr,
                reg.capacity_bytes,
                0, // available_bytes not in RegisterRequest
                0, // used_bytes not in RegisterRequest
                lease_expiry,
                Some(reg.node_version.as_str()),
            )
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        info!(
            "node registered: node_id={} addr={} capacity={}",
            node_id, reg.addr, reg.capacity_bytes
        );

        Ok(Response::new(RegisterResponse {
            node_id: node_id.to_string(),
            lease_ttl_ms,
            chunk_size_bytes: self.chunk_size_bytes,
            replication_factor: self.replication_factor,
        }))
    }

    async fn heartbeat(
        &self,
        request: Request<Streaming<HeartbeatRequest>>,
    ) -> Result<Response<Self::HeartbeatStream>, Status> {
        let mut stream = request.into_inner();
        let store = self.store.clone();

        // Process heartbeats as a stream: update lease on each heartbeat.
        let response_stream = async_stream::stream! {
            while let Ok(Some(hb)) = stream.message().await {
                let node_id = match Uuid::parse_str(&hb.node_id) {
                    Ok(id) => id,
                    Err(e) => {
                        yield Err(Status::invalid_argument(e.to_string()));
                        continue;
                    }
                };

                let lease_ttl_ms = 15_000i64;
                let lease_expiry =
                    chrono::Utc::now() + chrono::TimeDelta::try_seconds(15).unwrap();

                if let Err(e) = store.touch_node(
                    node_id,
                    hb.available_bytes,
                    hb.used_bytes,
                    lease_expiry,
                ).await {
                    yield Err(Status::internal(e.to_string()));
                    continue;
                }

                yield Ok(HeartbeatResponse {
                    lease_ttl_ms,
                    healthy: true,
                });
            }
        };

        Ok(Response::new(
            Box::pin(response_stream) as BoxStream<HeartbeatResponse>
        ))
    }

    async fn report_capacity(
        &self,
        request: Request<CapacityReport>,
    ) -> Result<Response<Empty>, Status> {
        let req = request.into_inner();
        let node_id =
            Uuid::parse_str(&req.node_id).map_err(|e| Status::invalid_argument(e.to_string()))?;

        let lease_expiry = chrono::Utc::now() + chrono::TimeDelta::try_seconds(15).unwrap();

        self.store
            .update_node_capacity(
                node_id,
                req.capacity_bytes,
                req.available_bytes,
                req.used_bytes,
                lease_expiry,
            )
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(Empty {}))
    }

    async fn check(
        &self,
        _request: Request<HealthCheckRequest>,
    ) -> Result<Response<HealthCheckResponse>, Status> {
        Ok(Response::new(HealthCheckResponse { serving: true }))
    }
}

#[tonic::async_trait]
impl ClusterService for CoordinatorService {
    async fn create_upload_session(
        &self,
        request: Request<CreateUploadSessionRequest>,
    ) -> Result<Response<CreateUploadSessionResponse>, Status> {
        let req = request.into_inner();
        let file_id = Uuid::new_v4();
        let upload_id = Uuid::new_v4();

        // The coordinator owns chunk size and replication factor: GetChunkPlan
        // places using these values, so the file row must record the same ones
        // (the gateway's request fields are advisory).
        let _ = self
            .store
            .create_file(
                file_id,
                &req.filename,
                req.total_size,
                self.chunk_size_bytes,
                0,
                self.replication_factor,
            )
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        let _ = self
            .store
            .create_upload_session(upload_id, Some(file_id))
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(CreateUploadSessionResponse {
            upload_id: upload_id.to_string(),
            file_id: file_id.to_string(),
            chunk_size: self.chunk_size_bytes,
            replication_factor: self.replication_factor,
        }))
    }

    async fn get_chunk_plan(
        &self,
        request: Request<GetChunkPlanRequest>,
    ) -> Result<Response<GetChunkPlanResponse>, Status> {
        let req = request.into_inner();
        let _upload_id =
            Uuid::parse_str(&req.upload_id).map_err(|e| Status::invalid_argument(e.to_string()))?;

        let nodes = self
            .store
            .healthy_nodes()
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        let node_ids = place_replicas(&nodes, 0, self.replication_factor, &[])
            .map_err(|e| Status::resource_exhausted(e.to_string()))?;

        let chunk_id = Uuid::new_v4();

        Ok(Response::new(GetChunkPlanResponse {
            chunk_id: chunk_id.to_string(),
            node_ids: node_ids.iter().map(|n| n.to_string()).collect(),
        }))
    }

    async fn complete_upload(
        &self,
        request: Request<CompleteUploadRequest>,
    ) -> Result<Response<CompleteUploadResponse>, Status> {
        let req = request.into_inner();
        let file_id =
            Uuid::parse_str(&req.file_id).map_err(|e| Status::invalid_argument(e.to_string()))?;
        let upload_id =
            Uuid::parse_str(&req.upload_id).map_err(|e| Status::invalid_argument(e.to_string()))?;

        let mut tx = self
            .store
            .pool()
            .begin()
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        for chunk_manifest in req.chunks {
            let chunk_id = Uuid::parse_str(&chunk_manifest.chunk_id)
                .map_err(|e| Status::invalid_argument(e.to_string()))?;

            // The gateway reports the nodes that durably stored the chunk.
            // Without replicas the manifest would be undownloadable and the
            // chunk would look under-replicated forever.
            if chunk_manifest.node_ids.is_empty() {
                return Err(Status::invalid_argument(
                    "chunk manifest is missing replica node_ids",
                ));
            }
            let node_ids = chunk_manifest
                .node_ids
                .iter()
                .map(|id| Uuid::parse_str(id))
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| Status::invalid_argument(e.to_string()))?;

            MetadataStore::insert_chunk_with_replicas(
                &mut tx,
                chunk_id,
                &chunk_manifest.sha256_hex,
                chunk_manifest.size_bytes,
                chunk_manifest.chunk_index as i32,
                file_id,
                &node_ids,
            )
            .await
            .map_err(|e| Status::internal(e.to_string()))?;
        }

        let _ = sqlx::query(
            "UPDATE files SET status='committed', updated_at=now() WHERE file_id=$1 AND status='uploading'",
        )
        .bind(file_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| Status::internal(e.to_string()))?;

        let _ = sqlx::query(
            "UPDATE upload_sessions SET status='committed', completed_at=now() WHERE upload_id=$1",
        )
        .bind(upload_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| Status::internal(e.to_string()))?;

        tx.commit()
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(CompleteUploadResponse {
            file_id: file_id.to_string(),
        }))
    }

    async fn get_manifest(
        &self,
        request: Request<GetManifestRequest>,
    ) -> Result<Response<GetManifestResponse>, Status> {
        let req = request.into_inner();
        let file_id =
            Uuid::parse_str(&req.file_id).map_err(|e| Status::invalid_argument(e.to_string()))?;

        let file = self
            .store
            .get_file(file_id)
            .await
            .map_err(|e| Status::internal(e.to_string()))?
            .ok_or_else(|| Status::not_found("file not found"))?;

        let chunks = self
            .store
            .chunks_for_file(file_id)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        let mut chunk_locations = Vec::new();
        for chunk in &chunks {
            let replicas = self
                .store
                .replicas_for_chunk(chunk.chunk_id)
                .await
                .map_err(|e| Status::internal(e.to_string()))?;
            let node_ids: Vec<String> = replicas
                .iter()
                .filter(|r| r.durable)
                .map(|r| r.node_id.to_string())
                .collect();

            chunk_locations.push(ChunkLocation {
                chunk_id: chunk.chunk_id.to_string(),
                chunk_index: chunk.chunk_index as i64,
                size_bytes: chunk.size_bytes,
                sha256_hex: chunk.sha256_hex.clone(),
                node_ids,
            });
        }

        Ok(Response::new(GetManifestResponse {
            file_id: file.file_id.to_string(),
            filename: file.filename,
            total_size: file.total_size,
            chunk_size: file.chunk_size,
            chunk_count: file.chunk_count,
            replication_factor: file.replication_factor,
            chunks: chunk_locations,
        }))
    }

    async fn delete_file(
        &self,
        request: Request<DeleteFileRequest>,
    ) -> Result<Response<DeleteFileResponse>, Status> {
        let req = request.into_inner();
        let file_id =
            Uuid::parse_str(&req.file_id).map_err(|e| Status::invalid_argument(e.to_string()))?;

        if self
            .store
            .get_file(file_id)
            .await
            .map_err(|e| Status::internal(e.to_string()))?
            .is_none()
        {
            return Err(Status::not_found("file not found"));
        }

        // Collect replica locations before the cascade delete removes them so
        // the caller can delete the chunk bytes from the storage nodes.
        let chunks = self
            .store
            .chunks_for_file(file_id)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        let mut locations = Vec::with_capacity(chunks.len());
        for chunk in &chunks {
            let replicas = self
                .store
                .replicas_for_chunk(chunk.chunk_id)
                .await
                .map_err(|e| Status::internal(e.to_string()))?;
            locations.push(ChunkLocation {
                chunk_id: chunk.chunk_id.to_string(),
                chunk_index: chunk.chunk_index as i64,
                size_bytes: chunk.size_bytes,
                sha256_hex: chunk.sha256_hex.clone(),
                node_ids: replicas.iter().map(|r| r.node_id.to_string()).collect(),
            });
        }

        // ON DELETE CASCADE removes chunks, chunk_replicas, replication_tasks,
        // and upload_sessions for this file.
        let result = sqlx::query("DELETE FROM files WHERE file_id=$1")
            .bind(file_id)
            .execute(self.store.pool())
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        if result.rows_affected() == 0 {
            return Err(Status::not_found("file not found"));
        }

        info!(
            "file deleted: file_id={} chunks={}",
            file_id,
            locations.len()
        );

        Ok(Response::new(DeleteFileResponse {
            deleted: true,
            chunks: locations,
        }))
    }

    async fn list_files(
        &self,
        request: Request<ListFilesRequest>,
    ) -> Result<Response<ListFilesResponse>, Status> {
        let req = request.into_inner();
        let limit = if req.limit <= 0 {
            DEFAULT_LIST_LIMIT
        } else {
            req.limit.min(MAX_LIST_LIMIT)
        };
        let after = if req.cursor.is_empty() {
            None
        } else {
            let (created_micros, file_id) = decode_file_cursor(&req.cursor)?;
            let created_at = chrono::DateTime::<chrono::Utc>::from_timestamp_micros(created_micros)
                .ok_or_else(|| Status::invalid_argument("invalid cursor"))?;
            Some((created_at, file_id))
        };

        // Fetch one extra row so we know whether a next page exists.
        let rows = self
            .store
            .list_files(i64::from(limit) + 1, after)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        let take = limit as usize;
        let has_more = rows.len() > take;
        let files: Vec<FileSummary> = rows
            .iter()
            .take(take)
            .map(|f| FileSummary {
                file_id: f.file_id.to_string(),
                filename: f.filename.clone(),
                total_size: f.total_size,
                chunk_count: i64::from(f.chunk_count),
                replication_factor: f.replication_factor,
                status: f.status.clone(),
                created_epoch_ms: f.created_at.timestamp_millis(),
            })
            .collect();

        let next_cursor = if has_more {
            rows.get(take - 1)
                .map(|f| encode_file_cursor(f.created_at, f.file_id))
                .unwrap_or_default()
        } else {
            String::new()
        };

        Ok(Response::new(ListFilesResponse { files, next_cursor }))
    }

    async fn list_nodes(
        &self,
        _request: Request<ListNodesRequest>,
    ) -> Result<Response<ListNodesResponse>, Status> {
        let nodes = self
            .store
            .all_nodes()
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(ListNodesResponse {
            nodes: nodes
                .into_iter()
                .map(|n| NodeInfo {
                    node_id: n.node_id.to_string(),
                    addr: n.addr,
                    grpc_addr: n.grpc_addr,
                    healthy: n.healthy,
                    available_bytes: n.available_bytes,
                    capacity_bytes: n.capacity_bytes,
                })
                .collect(),
        }))
    }

    async fn get_stats(
        &self,
        _request: Request<GetStatsRequest>,
    ) -> Result<Response<GetStatsResponse>, Status> {
        let nodes = self
            .store
            .all_nodes()
            .await
            .map_err(|e| Status::internal(e.to_string()))?;
        let healthy_nodes = nodes.iter().filter(|n| n.healthy).count() as i32;
        let total_nodes = nodes.len() as i32;
        let total_capacity: i64 = nodes.iter().map(|n| n.capacity_bytes).sum();
        let total_available: i64 = nodes.iter().map(|n| n.available_bytes).sum();

        Ok(Response::new(GetStatsResponse {
            total_nodes,
            healthy_nodes,
            total_capacity_bytes: total_capacity,
            total_available_bytes: total_available,
            total_files: 0,
            total_chunks: 0,
            under_replicated_chunks: 0,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::{
        decode_file_cursor, encode_file_cursor, node_endpoint, source_candidates,
        target_candidates, throttle_delay,
    };
    use chrono::{TimeZone, Utc};
    use metadata::models::ChunkReplicaRow;
    use std::collections::HashSet;
    use std::time::Duration;
    use uuid::Uuid;

    fn replica(node_id: Uuid, durable: bool) -> ChunkReplicaRow {
        ChunkReplicaRow {
            replica_id: Uuid::new_v4(),
            chunk_id: Uuid::new_v4(),
            node_id,
            durable,
            created_at: Utc::now(),
        }
    }

    #[test]
    fn file_cursor_roundtrips() {
        let created = Utc
            .timestamp_micros(1_700_000_000_123_456)
            .single()
            .expect("timestamp");
        let file_id = Uuid::new_v4();
        let cursor = encode_file_cursor(created, file_id);
        assert_eq!(
            decode_file_cursor(&cursor).expect("valid cursor"),
            (created.timestamp_micros(), file_id)
        );
    }

    #[test]
    fn file_cursor_rejects_garbage() {
        for cursor in [
            "missing-separator",
            "not-a-number:00000000-0000-0000-0000-000000000000",
            "123:not-a-uuid",
        ] {
            let err = decode_file_cursor(cursor).expect_err("invalid cursor");
            assert_eq!(err.code(), tonic::Code::InvalidArgument);
        }
    }

    #[test]
    fn throttle_delay_matches_configured_rate() {
        assert_eq!(throttle_delay(None, 1024), None);
        assert_eq!(throttle_delay(Some(0), 1024), None);
        assert_eq!(throttle_delay(Some(1000), 0), None);
        assert_eq!(
            throttle_delay(Some(1000), 500),
            Some(Duration::from_millis(500))
        );
        assert_eq!(
            throttle_delay(Some(1_000_000), 2_000_000),
            Some(Duration::from_secs(2))
        );
    }

    #[test]
    fn source_candidates_prefers_recorded_source() {
        let source = Uuid::new_v4();
        let other = Uuid::new_v4();
        let target = Uuid::new_v4();
        let healthy: HashSet<Uuid> = [source, other, target].into_iter().collect();
        let replicas = vec![replica(other, true), replica(source, true)];

        assert_eq!(
            source_candidates(Some(source), &replicas, &healthy, target),
            vec![source, other]
        );
    }

    #[test]
    fn source_candidates_falls_back_when_recorded_source_unhealthy() {
        let dead = Uuid::new_v4();
        let live = Uuid::new_v4();
        let target = Uuid::new_v4();
        let healthy: HashSet<Uuid> = [live, target].into_iter().collect();
        let replicas = vec![replica(dead, true), replica(live, true)];

        assert_eq!(
            source_candidates(Some(dead), &replicas, &healthy, target),
            vec![live]
        );
    }

    #[test]
    fn source_candidates_excludes_target_and_non_durable_replicas() {
        let live = Uuid::new_v4();
        let target = Uuid::new_v4();
        let healthy: HashSet<Uuid> = [live, target].into_iter().collect();
        let replicas = vec![replica(target, true), replica(live, false)];

        assert!(source_candidates(None, &replicas, &healthy, target).is_empty());
    }

    #[test]
    fn target_candidates_prefers_recorded_target() {
        let target = Uuid::new_v4();
        let spare = Uuid::new_v4();
        let source = Uuid::new_v4();
        let healthy = vec![spare, target, source];
        let replicas = vec![replica(source, true)];

        assert_eq!(
            target_candidates(target, &healthy, &replicas, source),
            vec![target, spare]
        );
    }

    #[test]
    fn target_candidates_skips_unhealthy_target_and_occupied_nodes() {
        let dead_target = Uuid::new_v4();
        let occupied = Uuid::new_v4();
        let spare = Uuid::new_v4();
        let source = Uuid::new_v4();
        let healthy = vec![occupied, source, spare];
        let replicas = vec![replica(source, true), replica(occupied, false)];

        assert_eq!(
            target_candidates(dead_target, &healthy, &replicas, source),
            vec![spare]
        );
    }

    #[test]
    fn target_candidates_empty_when_no_eligible_node() {
        let target = Uuid::new_v4();
        let source = Uuid::new_v4();
        let healthy = vec![source, target];
        let replicas = vec![replica(source, true), replica(target, true)];

        assert!(target_candidates(target, &healthy, &replicas, source).is_empty());
    }

    #[test]
    fn node_endpoint_adds_scheme_only_when_missing() {
        assert_eq!(node_endpoint("node:50051"), "http://node:50051");
        assert_eq!(node_endpoint("http://node:50051"), "http://node:50051");
        assert_eq!(node_endpoint("https://node:50051"), "https://node:50051");
    }
}
