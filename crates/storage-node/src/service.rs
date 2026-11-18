//! Storage node gRPC service implementation.
//!
//! Exposes ChunkService. Persists chunks via the storage engine (atomic writes
//! + SHA-256) and handles replication transfers.

use crate::StorageNode;
use futures_util::stream::Stream;
use proto::storage::{
    ChunkSummary, DeleteChunkRequest, DeleteChunkResponse, ListChunksRequest,
    ReplicateChunkRequest, ReplicateChunkResponse, RetrieveChunkRequest, RetrieveChunkResponse,
    StoreChunkRequest, StoreChunkResponse,
    chunk_service_server::{ChunkService, ChunkServiceServer},
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
impl ChunkService for StorageNodeService {
    type RetrieveChunkStream = BoxStream<RetrieveChunkResponse>;
    type ListChunksStream = BoxStream<ChunkSummary>;

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

        observability::STORAGE_CHUNKS_STORED.inc();
        observability::STORAGE_BYTES_STORED.inc_by(data.len() as u64);

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

        if deleted {
            observability::STORAGE_CHUNKS_DELETED.inc();
        }

        Ok(Response::new(DeleteChunkResponse { deleted }))
    }

    async fn replicate_chunk(
        &self,
        _request: Request<Streaming<ReplicateChunkRequest>>,
    ) -> Result<Response<ReplicateChunkResponse>, Status> {
        Err(Status::unimplemented("replicate_chunk not yet implemented"))
    }

    async fn list_chunks(
        &self,
        _request: Request<ListChunksRequest>,
    ) -> Result<Response<Self::ListChunksStream>, Status> {
        let chunks = self
            .node
            .list_chunks()
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        let stream = async_stream::stream! {
            for chunk in chunks {
                yield Ok(ChunkSummary {
                    chunk_id: chunk.chunk_id,
                    size_bytes: chunk.size_bytes as i64,
                    modified_epoch_ms: chunk.modified_epoch_ms,
                });
            }
        };

        Ok(Response::new(Box::pin(stream) as BoxStream<ChunkSummary>))
    }
}

pub struct StorageNodeServer;

impl StorageNodeServer {
    pub async fn run(
        addr: std::net::SocketAddr,
        node: Arc<StorageNode>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        Self::run_with_shutdown(addr, node, common::shutdown::signal()).await
    }

    /// Serve until `shutdown` resolves, then stop accepting new requests and
    /// let in-flight requests finish.
    pub async fn run_with_shutdown(
        addr: std::net::SocketAddr,
        node: Arc<StorageNode>,
        shutdown: impl std::future::Future<Output = ()> + Send + 'static,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let listener = tokio::net::TcpListener::bind(addr).await?;
        let local = listener.local_addr()?;
        tracing::info!("storage node gRPC listening on {local}");
        Self::serve_with_listener_shutdown(listener, node, shutdown).await
    }

    /// Serve on an already-bound listener (used by tests to pick an ephemeral
    /// port).
    pub async fn serve_with_listener_shutdown(
        listener: tokio::net::TcpListener,
        node: Arc<StorageNode>,
        shutdown: impl std::future::Future<Output = ()> + Send + 'static,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let svc = StorageNodeService::new(node);
        tonic::transport::Server::builder()
            .add_service(ChunkServiceServer::new(svc))
            .serve_with_incoming_shutdown(
                tokio_stream::wrappers::TcpListenerStream::new(listener),
                shutdown,
            )
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proto::storage::{
        ChunkReference, DeleteChunkRequest, ListChunksRequest, RetrieveChunkRequest,
        StoreChunkRequest, chunk_service_client::ChunkServiceClient,
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
        assert!(observability::STORAGE_CHUNKS_STORED.get() >= 1);
        assert!(
            observability::STORAGE_BYTES_STORED.get() >= data.len() as u64,
            "stored byte counter should include the chunk"
        );

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
        assert!(
            observability::STORAGE_CHUNKS_DELETED.get() >= 1,
            "deleted chunk counter should increase"
        );

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

    #[tokio::test]
    async fn grpc_list_chunks_reports_stored_chunks() {
        let node = start_node().await;
        let mut client = connect(node.addr).await;

        for (chunk_id, data) in [
            ("list-chunk-a", b"aaa".to_vec()),
            ("list-chunk-b", b"bbbbbb".to_vec()),
        ] {
            let sha = sha256_hex(&data);
            client
                .store_chunk(store_stream(chunk_id, &data, &sha))
                .await
                .expect("store");
        }

        let mut stream = client
            .list_chunks(ListChunksRequest {})
            .await
            .expect("list chunks")
            .into_inner();

        let mut listed = Vec::new();
        while let Some(chunk) = stream.message().await.expect("stream message") {
            listed.push((chunk.chunk_id, chunk.size_bytes, chunk.modified_epoch_ms));
        }
        listed.sort();
        assert_eq!(listed.len(), 2, "both stored chunks must be listed");
        assert_eq!(listed[0].0, "list-chunk-a");
        assert_eq!(listed[0].1, 3);
        assert_eq!(listed[1].0, "list-chunk-b");
        assert_eq!(listed[1].1, 6);
        assert!(
            listed.iter().all(|(_, _, mtime)| *mtime > 0),
            "listings must carry a modification time"
        );
    }

    #[tokio::test]
    async fn graceful_shutdown_stops_server() {
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

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let addr = listener.local_addr().expect("addr");
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        let server = tokio::spawn(async move {
            let _ = StorageNodeServer::serve_with_listener_shutdown(listener, node, async {
                let _ = rx.await;
            })
            .await;
        });

        tokio::time::sleep(Duration::from_millis(100)).await;

        let mut client = ChunkServiceClient::connect(format!("http://{addr}"))
            .await
            .expect("connect");
        client
            .list_chunks(ListChunksRequest {})
            .await
            .expect("list chunks");

        drop(client);
        tx.send(()).expect("send shutdown");

        let joined = tokio::time::timeout(Duration::from_secs(3), server).await;
        assert!(joined.is_ok(), "server did not stop after shutdown signal");
        joined.expect("join").expect("task panicked");
    }
}
