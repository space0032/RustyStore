//! Coordinator gRPC service implementation.
//!
//! Implements NodeService, ChunkService, and ClusterService from the proto
//! contract. Phase 3 adds gateway-facing RPCs: upload sessions, chunk plans,
//! complete upload, manifest lookup, node listing, and cluster stats.

use crate::placement::place_replicas;
use futures_util::stream::Stream;
use metadata::MetadataStore;
use proto::storage::{
    CapacityReport, ChunkLocation, CompleteUploadRequest, CompleteUploadResponse,
    CreateUploadSessionRequest, CreateUploadSessionResponse, Empty, GetChunkPlanRequest,
    GetChunkPlanResponse, GetManifestRequest, GetManifestResponse, GetStatsRequest,
    GetStatsResponse, HealthCheckRequest, HealthCheckResponse, HeartbeatRequest, HeartbeatResponse,
    ListNodesRequest, ListNodesResponse, NodeInfo, RegisterRequest, RegisterResponse,
    cluster_service_server::{ClusterService, ClusterServiceServer},
    node_service_server::NodeService,
};
use std::pin::Pin;
use std::sync::Arc;
use tonic::{Request, Response, Status, Streaming};
use uuid::Uuid;

#[derive(Clone)]
pub struct CoordinatorService {
    store: Arc<MetadataStore>,
    replication_factor: i32,
    #[allow(dead_code)]
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
    type HeartbeatStream = Pin<Box<dyn Stream<Item = Result<HeartbeatResponse, Status>> + Send>>;

    async fn register(
        &self,
        _request: Request<Streaming<RegisterRequest>>,
    ) -> Result<Response<RegisterResponse>, Status> {
        Err(Status::unimplemented("register not yet implemented"))
    }

    async fn heartbeat(
        &self,
        _request: Request<Streaming<HeartbeatRequest>>,
    ) -> Result<Response<Self::HeartbeatStream>, Status> {
        Err(Status::unimplemented("heartbeat not yet implemented"))
    }

    async fn report_capacity(
        &self,
        _request: Request<CapacityReport>,
    ) -> Result<Response<Empty>, Status> {
        Err(Status::unimplemented("report_capacity not yet implemented"))
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
    type NodeHeartbeatStream =
        Pin<Box<dyn Stream<Item = Result<HeartbeatResponse, Status>> + Send>>;

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
        _request: Request<Streaming<proto::storage::ReplicateChunkRequest>>,
    ) -> Result<Response<proto::storage::ReplicateChunkResponse>, Status> {
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
