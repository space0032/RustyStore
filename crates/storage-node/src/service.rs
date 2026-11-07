//! Storage node gRPC service implementation.
//!
//! Exposes NodeService, ChunkService, and ClusterService. Persists chunks via
//! the storage engine (atomic writes + SHA-256), reports capacity, and
//! handles replication transfers.

use futures_util::stream::Stream;
use proto::storage::{
    CapacityReport, CompleteUploadRequest, CompleteUploadResponse, CreateUploadSessionRequest,
    CreateUploadSessionResponse, DeleteChunkRequest, DeleteChunkResponse, Empty,
    GetChunkPlanRequest, GetChunkPlanResponse, GetManifestRequest, GetManifestResponse,
    GetStatsRequest, GetStatsResponse, HealthCheckRequest, HealthCheckResponse, HeartbeatRequest,
    HeartbeatResponse, ListNodesRequest, ListNodesResponse, RegisterRequest, RegisterResponse,
    ReplicateChunkRequest, ReplicateChunkResponse, RetrieveChunkRequest, RetrieveChunkResponse,
    StoreChunkRequest, StoreChunkResponse, chunk_service_server::ChunkService,
    cluster_service_server::ClusterService, node_service_server::NodeService,
};
use std::pin::Pin;
use tonic::{Request, Response, Status, Streaming};

#[allow(dead_code)]
type BoxStream<T> = Pin<Box<dyn Stream<Item = Result<T, Status>> + Send>>;

#[allow(dead_code)]
pub struct StorageNodeService;

#[tonic::async_trait]
impl NodeService for StorageNodeService {
    type HeartbeatStream = BoxStream<HeartbeatResponse>;

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
impl ChunkService for StorageNodeService {
    type RetrieveChunkStream = BoxStream<RetrieveChunkResponse>;

    async fn store_chunk(
        &self,
        _request: Request<Streaming<StoreChunkRequest>>,
    ) -> Result<Response<StoreChunkResponse>, Status> {
        Err(Status::unimplemented("store_chunk not yet implemented"))
    }

    async fn retrieve_chunk(
        &self,
        _request: Request<RetrieveChunkRequest>,
    ) -> Result<Response<Self::RetrieveChunkStream>, Status> {
        Err(Status::unimplemented("retrieve_chunk not yet implemented"))
    }

    async fn delete_chunk(
        &self,
        _request: Request<DeleteChunkRequest>,
    ) -> Result<Response<DeleteChunkResponse>, Status> {
        Err(Status::unimplemented("delete_chunk not yet implemented"))
    }

    async fn replicate_chunk(
        &self,
        _request: Request<Streaming<ReplicateChunkRequest>>,
    ) -> Result<Response<ReplicateChunkResponse>, Status> {
        Err(Status::unimplemented("replicate_chunk not yet implemented"))
    }
}

#[tonic::async_trait]
impl ClusterService for StorageNodeService {
    type NodeHeartbeatStream = BoxStream<HeartbeatResponse>;

    async fn assign_replica(
        &self,
        _request: Request<Streaming<ReplicateChunkRequest>>,
    ) -> Result<Response<ReplicateChunkResponse>, Status> {
        Err(Status::unimplemented("assign_replica not yet implemented"))
    }

    async fn create_upload_session(
        &self,
        _request: Request<CreateUploadSessionRequest>,
    ) -> Result<Response<CreateUploadSessionResponse>, Status> {
        Err(Status::unimplemented(
            "create_upload_session not yet implemented",
        ))
    }

    async fn get_chunk_plan(
        &self,
        _request: Request<GetChunkPlanRequest>,
    ) -> Result<Response<GetChunkPlanResponse>, Status> {
        Err(Status::unimplemented("get_chunk_plan not yet implemented"))
    }

    async fn complete_upload(
        &self,
        _request: Request<CompleteUploadRequest>,
    ) -> Result<Response<CompleteUploadResponse>, Status> {
        Err(Status::unimplemented("complete_upload not yet implemented"))
    }

    async fn get_manifest(
        &self,
        _request: Request<GetManifestRequest>,
    ) -> Result<Response<GetManifestResponse>, Status> {
        Err(Status::unimplemented("get_manifest not yet implemented"))
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
        Err(Status::unimplemented("list_nodes not yet implemented"))
    }

    async fn get_stats(
        &self,
        _request: Request<GetStatsRequest>,
    ) -> Result<Response<GetStatsResponse>, Status> {
        Err(Status::unimplemented("get_stats not yet implemented"))
    }
}

#[allow(dead_code)]
pub struct StorageNodeServer;

impl StorageNodeServer {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self
    }
}
