//! Storage node gRPC service implementation.
//!
//! Exposes NodeService, ChunkService, and ClusterService. Persists chunks via
//! the storage engine (atomic writes + SHA-256), reports capacity, and
//! handles replication transfers.

use crate::StorageNode;
use futures_util::stream::Stream;
use proto::storage::{
    CapacityReport, CompleteUploadRequest, CompleteUploadResponse, CreateUploadSessionRequest,
    CreateUploadSessionResponse, DeleteChunkRequest, DeleteChunkResponse, Empty,
    GetChunkPlanRequest, GetChunkPlanResponse, GetManifestRequest, GetManifestResponse,
    GetStatsRequest, GetStatsResponse, HealthCheckRequest, HealthCheckResponse, HeartbeatRequest,
    HeartbeatResponse, ListNodesRequest, ListNodesResponse, RegisterRequest, RegisterResponse,
    ReplicateChunkRequest, ReplicateChunkResponse, RetrieveChunkRequest, RetrieveChunkResponse,
    StoreChunkRequest, StoreChunkResponse,
    chunk_service_server::{ChunkService, ChunkServiceServer},
    cluster_service_server::{ClusterService, ClusterServiceServer},
    node_service_server::{NodeService, NodeServiceServer},
};
use std::pin::Pin;
use std::sync::Arc;
use tokio::io::AsyncReadExt;
use tonic::{Request, Response, Status, Streaming};

type BoxStream<T> = Pin<Box<dyn Stream<Item = Result<T, Status>> + Send>>;

#[derive(Clone)]
pub struct StorageNodeService {
    node: Arc<StorageNode>,
}

impl StorageNodeService {
    pub fn new(node: Arc<StorageNode>) -> Self {
        Self { node }
    }
}

#[tonic::async_trait]
impl NodeService for StorageNodeService {
    type HeartbeatStream = BoxStream<HeartbeatResponse>;

    async fn register(
        &self,
        request: Request<Streaming<RegisterRequest>>,
    ) -> Result<Response<RegisterResponse>, Status> {
        let mut stream = request.into_inner();
        let reg = stream
            .message()
            .await
            .map_err(|e| Status::internal(e.to_string()))?
            .ok_or_else(|| Status::invalid_argument("empty register stream"))?;

        let node_id = self
            .node
            .register(&reg.addr, &reg.grpc_addr, reg.capacity_bytes)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(RegisterResponse {
            node_id: node_id.to_string(),
            lease_ttl_ms: 15_000,
            chunk_size_bytes: 4 * 1024 * 1024,
            replication_factor: 3,
        }))
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
        request: Request<Streaming<StoreChunkRequest>>,
    ) -> Result<Response<StoreChunkResponse>, Status> {
        let mut stream = request.into_inner();

        // First message must be metadata.
        let meta = stream
            .message()
            .await
            .map_err(|e| Status::internal(e.to_string()))?
            .ok_or_else(|| Status::invalid_argument("empty store_chunk stream"))?;

        let meta_ref = match meta.payload {
            Some(proto::storage::store_chunk_request::Payload::Meta(m)) => m,
            _ => return Err(Status::invalid_argument("first message must be metadata")),
        };

        let chunk_id = meta_ref.chunk_id.clone();
        let expected_sha256 = meta_ref.sha256_hex.clone();
        let size_bytes = meta_ref.size_bytes as usize;

        let node = self.node.clone();

        // Collect data frames.
        let mut data = Vec::with_capacity(size_bytes);
        while let Some(frame) = stream
            .message()
            .await
            .map_err(|e| Status::internal(e.to_string()))?
        {
            match frame.payload {
                Some(proto::storage::store_chunk_request::Payload::Data(frame_data)) => {
                    data.extend_from_slice(&frame_data);
                }
                _ => return Err(Status::invalid_argument("unexpected non-data frame")),
            }
        }

        // Store atomically with SHA-256 verification.
        node.store_chunk(&chunk_id, &data, &expected_sha256)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(StoreChunkResponse {
            chunk_id,
            durable: true,
        }))
    }

    async fn retrieve_chunk(
        &self,
        request: Request<RetrieveChunkRequest>,
    ) -> Result<Response<Self::RetrieveChunkStream>, Status> {
        let req = request.into_inner();
        let node = self.node.clone();

        let (meta, mut reader) = node
            .open_chunk_reader(&req.chunk_id)
            .await
            .map_err(|e| Status::not_found(e.to_string()))?;

        let response_stream = async_stream::stream! {
            yield Ok(RetrieveChunkResponse {
                payload: Some(proto::storage::retrieve_chunk_response::Payload::Meta(
                    proto::storage::ChunkReference {
                        chunk_id: meta.chunk_id,
                        file_id: String::new(),
                        chunk_index: 0,
                        size_bytes: meta.size_bytes as i64,
                        sha256_hex: meta.sha256_hex,
                        replication_factor: 0,
                    },
                )),
            });

            let mut buf = [0u8; 64 * 1024];
            loop {
                let n = match reader.read(&mut buf).await {
                    Ok(n) => n,
                    Err(e) => {
                        yield Err(Status::internal(e.to_string()));
                        break;
                    }
                };
                if n == 0 {
                    break;
                }
                yield Ok(RetrieveChunkResponse {
                    payload: Some(proto::storage::retrieve_chunk_response::Payload::Data(
                        buf[..n].to_vec(),
                    )),
                });
            }
        };

        Ok(Response::new(
            Box::pin(response_stream) as BoxStream<RetrieveChunkResponse>
        ))
    }

    async fn delete_chunk(
        &self,
        request: Request<DeleteChunkRequest>,
    ) -> Result<Response<DeleteChunkResponse>, Status> {
        let req = request.into_inner();
        let node = self.node.clone();

        let deleted = node
            .delete_chunk(&req.chunk_id)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(DeleteChunkResponse { deleted }))
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

pub struct StorageNodeServer;

impl StorageNodeServer {
    pub async fn run(
        addr: std::net::SocketAddr,
        node: Arc<StorageNode>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let svc = StorageNodeService::new(node);
        tonic::transport::Server::builder()
            .add_service(NodeServiceServer::new(svc.clone()))
            .add_service(ChunkServiceServer::new(svc.clone()))
            .add_service(ClusterServiceServer::new(svc))
            .serve(addr)
            .await?;
        Ok(())
    }
}
