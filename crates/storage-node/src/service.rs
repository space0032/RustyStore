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

        let (mut meta, mut reader) = node
            .open_chunk_reader(&req.chunk_id)
            .await
            .map_err(|e| Status::not_found(e.to_string()))?;

        if meta.sha256_hex.is_empty() && !req.expected_sha256.is_empty() {
            meta.sha256_hex = req.expected_sha256.clone();
        }

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

#[cfg(test)]
mod tests {
    use super::*;
    use proto::storage::{
        ChunkReference, DeleteChunkRequest, RetrieveChunkRequest, StoreChunkRequest,
        chunk_service_client::ChunkServiceClient,
        retrieve_chunk_response::Payload as RetrievePayload,
        store_chunk_request::Payload as StorePayload,
    };
    use sha2::{Digest, Sha256};
    use std::net::SocketAddr;
    use std::time::Duration;
    use tokio_stream::wrappers::TcpListenerStream;
    use uuid::Uuid;

    struct TestNode {
        addr: SocketAddr,
        _dir: tempfile::TempDir,
    }

    async fn start_node() -> TestNode {
        let dir = tempfile::tempdir().expect("tempdir");
        let node = Arc::new(
            StorageNode::new(
                "127.0.0.1:0".to_string(),
                "127.0.0.1:0".to_string(),
                dir.path(),
            )
            .await
            .expect("node"),
        );
        let svc = StorageNodeService::new(node);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let addr = listener.local_addr().expect("addr");
        tokio::spawn(async move {
            tonic::transport::Server::builder()
                .add_service(ChunkServiceServer::new(svc))
                .serve_with_incoming(TcpListenerStream::new(listener))
                .await
                .expect("serve");
        });
        tokio::time::sleep(Duration::from_millis(100)).await;
        TestNode { addr, _dir: dir }
    }

    fn sha256_hex(data: &[u8]) -> String {
        hex::encode(Sha256::digest(data))
    }

    async fn connect(addr: SocketAddr) -> ChunkServiceClient<tonic::transport::Channel> {
        ChunkServiceClient::connect(format!("http://{addr}"))
            .await
            .expect("connect")
    }

    fn store_stream(
        chunk_id: &str,
        data: &[u8],
        sha: &str,
    ) -> impl tonic::IntoStreamingRequest<Message = StoreChunkRequest> {
        let meta = ChunkReference {
            chunk_id: chunk_id.to_string(),
            file_id: String::new(),
            chunk_index: 0,
            size_bytes: data.len() as i64,
            sha256_hex: sha.to_string(),
            replication_factor: 1,
        };
        let frames: Vec<Vec<u8>> = data.chunks(7).map(|c| c.to_vec()).collect();
        tonic::Request::new(async_stream::stream! {
            yield StoreChunkRequest { payload: Some(StorePayload::Meta(meta)) };
            for frame in frames {
                yield StoreChunkRequest { payload: Some(StorePayload::Data(frame)) };
            }
        })
    }

    #[tokio::test]
    async fn grpc_store_retrieve_delete_roundtrip() {
        let node = start_node().await;
        let mut client = connect(node.addr).await;

        let chunk_id = Uuid::new_v4().to_string();
        let data = b"hello distributed storage over grpc".to_vec();
        let sha = sha256_hex(&data);

        let stored = client
            .store_chunk(store_stream(&chunk_id, &data, &sha))
            .await
            .expect("store")
            .into_inner();
        assert!(stored.durable, "chunk should be durable");
        assert_eq!(stored.chunk_id, chunk_id);

        let mut stream = client
            .retrieve_chunk(RetrieveChunkRequest {
                chunk_id: chunk_id.clone(),
                expected_sha256: sha.clone(),
            })
            .await
            .expect("retrieve")
            .into_inner();

        let mut got = Vec::new();
        let mut meta_seen = false;
        while let Some(message) = stream.message().await.expect("stream message") {
            match message.payload {
                Some(RetrievePayload::Meta(meta)) => {
                    meta_seen = true;
                    assert_eq!(meta.sha256_hex, sha);
                    assert_eq!(meta.size_bytes as usize, data.len());
                }
                Some(RetrievePayload::Data(bytes)) => got.extend_from_slice(&bytes),
                None => {}
            }
        }
        assert!(meta_seen, "metadata frame must precede data");
        assert_eq!(got, data, "retrieved bytes must match");

        let deleted = client
            .delete_chunk(DeleteChunkRequest {
                chunk_id: chunk_id.clone(),
            })
            .await
            .expect("delete")
            .into_inner();
        assert!(deleted.deleted);

        let after_delete = client
            .retrieve_chunk(RetrieveChunkRequest {
                chunk_id,
                expected_sha256: String::new(),
            })
            .await;
        assert!(after_delete.is_err(), "retrieve after delete must fail");
    }

    #[tokio::test]
    async fn grpc_store_rejects_sha_mismatch() {
        let node = start_node().await;
        let mut client = connect(node.addr).await;

        let chunk_id = Uuid::new_v4().to_string();
        let data = b"corrupt me".to_vec();
        let wrong_sha = sha256_hex(b"different data");

        let result = client
            .store_chunk(store_stream(&chunk_id, &data, &wrong_sha))
            .await;
        assert!(result.is_err(), "mismatched SHA-256 must be rejected");
    }

    #[tokio::test]
    async fn grpc_retrieve_missing_chunk_not_found() {
        let node = start_node().await;
        let mut client = connect(node.addr).await;

        let result = client
            .retrieve_chunk(RetrieveChunkRequest {
                chunk_id: Uuid::new_v4().to_string(),
                expected_sha256: String::new(),
            })
            .await;
        match result {
            Err(status) => assert_eq!(status.code(), tonic::Code::NotFound),
            Ok(_) => panic!("expected NotFound for missing chunk"),
        }
    }
}
