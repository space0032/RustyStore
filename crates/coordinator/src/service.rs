//! Coordinator gRPC service implementation + background tasks.
//!
//! Implements NodeService (Register, Heartbeat, ReportCapacity, Check) and
//! ClusterService (gateway-facing RPCs). Runs background tasks for lease
//! expiry monitoring, under-replication detection, and recovery scheduling.

use crate::placement::place_replicas;
#[allow(unused_imports)]
use crate::types::{ChunkLocationEntry, ChunkManifestEntry, ClusterStats, Manifest, NodeInfoEntry};
use futures_util::stream::Stream;
use metadata::MetadataStore;
use metadata::models::StorageNodeRow;
use proto::storage::{
    CapacityReport, ChunkLocation, CompleteUploadRequest, CompleteUploadResponse,
    CreateUploadSessionRequest, CreateUploadSessionResponse, Empty, GetChunkPlanRequest,
    GetChunkPlanResponse, GetManifestRequest, GetManifestResponse, GetStatsRequest,
    GetStatsResponse, HealthCheckRequest, HealthCheckResponse, HeartbeatRequest, HeartbeatResponse,
    ListNodesRequest, ListNodesResponse, NodeInfo, RegisterRequest, RegisterResponse,
    ReplicateChunkRequest, ReplicateChunkResponse,
    cluster_service_server::{ClusterService, ClusterServiceServer},
    node_service_server::NodeService,
};
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use tokio::task::JoinHandle;
use tonic::{Request, Response, Status, Streaming};
use tracing::{info, warn};
use uuid::Uuid;

type BoxStream<T> = Pin<Box<dyn Stream<Item = Result<T, Status>> + Send>>;

/// The Coordinator: gRPC service + background recovery tasks.
pub struct Coordinator {
    service: CoordinatorService,
    #[allow(dead_code)]
    lease_monitor_handle: Option<JoinHandle<()>>,
    #[allow(dead_code)]
    replication_monitor_handle: Option<JoinHandle<()>>,
    #[allow(dead_code)]
    recovery_scheduler_handle: Option<JoinHandle<()>>,
}

impl Coordinator {
    pub fn new(store: Arc<MetadataStore>, replication_factor: i32, chunk_size_bytes: i64) -> Self {
        let service = CoordinatorService::new(store.clone(), replication_factor, chunk_size_bytes);

        // Lease expiry monitor: marks nodes unhealthy when lease expires.
        let lease_monitor = {
            let store = store.clone();
            let interval = Duration::from_secs(5);
            tokio::spawn(async move {
                let mut interval = tokio::time::interval(interval);
                loop {
                    interval.tick().await;
                    if let Err(e) = store.expire_leases().await {
                        warn!("lease expiry monitor error: {e}");
                    }
                }
            })
        };

        // Under-replication detector: finds under-replicated chunks and
        // creates replication tasks.
        let replication_monitor = {
            let store = store.clone();
            let rf = replication_factor;
            let interval = Duration::from_secs(10);
            tokio::spawn(async move {
                let mut interval = tokio::time::interval(interval);
                loop {
                    interval.tick().await;
                    if let Err(e) = Self::detect_under_replicated(&store, rf).await {
                        warn!("under-replication detector error: {e}");
                    }
                }
            })
        };

        // Recovery scheduler: claims replication tasks and executes them
        // with bounded concurrency.
        let recovery_scheduler = {
            let store = store.clone();
            let interval = Duration::from_secs(5);
            tokio::spawn(async move {
                let mut interval = tokio::time::interval(interval);
                loop {
                    interval.tick().await;
                    if let Err(e) = Self::run_recovery_cycle(&store).await {
                        warn!("recovery scheduler error: {e}");
                    }
                }
            })
        };

        Self {
            service,
            lease_monitor_handle: Some(lease_monitor),
            replication_monitor_handle: Some(replication_monitor),
            recovery_scheduler_handle: Some(recovery_scheduler),
        }
    }

    /// Detect under-replicated chunks and create replication tasks.
    async fn detect_under_replicated(
        store: &Arc<MetadataStore>,
        _default_rf: i32,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let under_replicated = store.under_replicated_chunks().await?;

        for (chunk_id, _file_id, _required, _actual) in under_replicated {
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
                store
                    .create_replication_task(
                        task_id,
                        chunk_id,
                        Some(source_node_id),
                        *target_node_id,
                        5,
                    )
                    .await?;
                info!(
                    "created replication task for chunk {chunk_id}: node {source_node_id} -> {}",
                    target_node_id
                );
            }
        }

        Ok(())
    }

    /// Run one recovery cycle: claim a task and execute it.
    async fn run_recovery_cycle(
        store: &Arc<MetadataStore>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Claim a pending task.
        let Some(task) = store.claim_replication_task().await? else {
            return Ok(()); // no pending tasks
        };

        let chunk_id = task.chunk_id;
        let target_node_id = task.target_node_id;

        // Get chunk info for the replication.
        let replicas = store.replicas_for_chunk(chunk_id).await?;
        let has_durable_replica = replicas.iter().any(|r| r.durable);

        if !has_durable_replica {
            warn!(
                "chunk {chunk_id} has no durable replica, skipping task {}",
                task.task_id
            );
            store
                .mark_task_failed(task.task_id, task.attempts + 1)
                .await?;
            return Ok(());
        }

        // TODO: In Phase 5/6, actually perform the gRPC replication transfer
        // from source to target node. For Phase 4, mark the task as done
        // after recording the replica intent.
        // Record the replica placement intent (durable=false until confirmed
        // by the target node in Phase 5/6).
        store
            .insert_replica(chunk_id, target_node_id, false)
            .await?;
        store.mark_task_done(task.task_id).await?;

        info!(
            "recovery task {} complete: chunk {} -> node {}",
            task.task_id, chunk_id, target_node_id
        );

        Ok(())
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

                if let Err(e) = store.upsert_node(
                    node_id,
                    "", // addr not updated via heartbeat
                    "", // grpc_addr not updated via heartbeat
                    0,  // capacity not updated via heartbeat
                    hb.available_bytes,
                    hb.used_bytes,
                    lease_expiry,
                    None,
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
            .upsert_node(
                node_id,
                "",
                "",
                req.capacity_bytes,
                req.available_bytes,
                req.used_bytes,
                lease_expiry,
                None,
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
    type NodeHeartbeatStream = BoxStream<HeartbeatResponse>;

    async fn create_upload_session(
        &self,
        request: Request<CreateUploadSessionRequest>,
    ) -> Result<Response<CreateUploadSessionResponse>, Status> {
        let req = request.into_inner();
        let file_id = Uuid::new_v4();
        let upload_id = Uuid::new_v4();

        let _ = self
            .store
            .create_file(
                file_id,
                &req.filename,
                req.total_size,
                req.chunk_size,
                0,
                req.replication_factor,
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

            let _ = self
                .store
                .insert_chunk(
                    chunk_id,
                    &chunk_manifest.sha256_hex,
                    chunk_manifest.size_bytes,
                    chunk_manifest.chunk_index as i32,
                    file_id,
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

    async fn assign_replica(
        &self,
        _request: Request<Streaming<ReplicateChunkRequest>>,
    ) -> Result<Response<ReplicateChunkResponse>, Status> {
        Err(Status::unimplemented("assign_replica not yet implemented"))
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

    async fn node_heartbeat(
        &self,
        _request: Request<Streaming<HeartbeatRequest>>,
    ) -> Result<Response<Self::NodeHeartbeatStream>, Status> {
        Err(Status::unimplemented("node_heartbeat not yet implemented"))
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

#[allow(dead_code)]
pub struct CoordinatorServer;

impl CoordinatorServer {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self
    }
}
