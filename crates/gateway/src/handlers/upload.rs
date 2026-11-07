//! File upload handler.
//! POST /api/v1/files
//!
//! Streaming upload: the request body is split into bounded chunks, each
//! hashed, placed, and replicated as it arrives. Never buffers the entire
//! file in memory.

use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::HeaderMap;
use axum::response::Json;
use futures_util::StreamExt;
use std::sync::Arc;
use tracing::info;

use crate::AppState;
use crate::chunk_utils::ChunkGenerator;

#[derive(serde::Serialize)]
pub struct UploadResponse {
    pub file_id: String,
    pub upload_id: String,
    pub chunk_size: i64,
    pub chunk_count: u64,
    pub replication_factor: i32,
}

const DEFAULT_CHUNK_SIZE: i64 = 4 * 1024 * 1024; // 4 MB

fn replication_factor() -> i32 {
    3
}

pub async fn upload_file(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    req: Request<Body>,
) -> Result<Json<UploadResponse>, crate::error::GatewayError> {
    let filename = headers
        .get("x-filename")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("upload")
        .to_string();

    let total_size: i64 = headers
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);

    let chunk_size = DEFAULT_CHUNK_SIZE as usize;
    let rf = replication_factor();

    // Create upload session with coordinator.
    let mut coord_client = state.coord_client().await?;
    let (upload_id, file_id) = coord_client
        .create_upload_session(&filename, total_size, chunk_size as i64, rf)
        .await
        .map_err(|e| crate::error::GatewayError::Grpc(e.to_string()))?;

    info!(
        "upload started: file_id={} upload_id={} filename={} total_size={} chunk_size={} rf={}",
        file_id, upload_id, filename, total_size, chunk_size, rf
    );

    // Build node_id -> grpc_addr map.
    let nodes = coord_client
        .list_nodes()
        .await
        .map_err(|e| crate::error::GatewayError::Grpc(e.to_string()))?;
    let node_addrs: std::collections::HashMap<uuid::Uuid, String> = nodes
        .into_iter()
        .map(|n| (n.node_id, n.grpc_addr))
        .collect();

    // Wrap the request body stream as AsyncRead for ChunkGenerator.
    let body_stream = req.into_body().into_data_stream();
    let stream = futures_util::stream::StreamExt::map(
        body_stream,
        |r: Result<bytes::Bytes, axum::Error>| r.map_err(|e| std::io::Error::other(e.to_string())),
    );
    let async_read = StreamReader::new(stream.boxed());
    let mut r#gen = ChunkGenerator::new(async_read, chunk_size);

    let mut chunk_count: u64 = 0;
    let mut chunk_manifests: Vec<coordinator::types::ChunkManifestEntry> = Vec::new();

    loop {
        let chunk = r#gen.next_chunk().await.map_err(|e| {
            crate::error::GatewayError::Internal(format!("chunk generation error: {e}"))
        })?;
        let Some(chunk) = chunk else {
            break;
        };

        chunk_count += 1;

        // Get chunk placement from coordinator.
        let (chunk_id_str, node_ids) = coord_client
            .get_chunk_plan(upload_id, chunk.index as i64)
            .await
            .map_err(|e| crate::error::GatewayError::Grpc(e.to_string()))?;

        let chunk_id = uuid::Uuid::parse_str(&chunk_id_str)
            .map_err(|e| crate::error::GatewayError::InvalidArgument(e.to_string()))?;

        // Store chunk to all replicas concurrently.
        store_chunk_to_replicas(
            &node_addrs,
            &node_ids,
            &chunk_id,
            &chunk.data,
            &chunk.sha256_hex,
        )
        .await
        .map_err(|e| crate::error::GatewayError::Internal(format!("replica storage error: {e}")))?;

        chunk_manifests.push(coordinator::types::ChunkManifestEntry {
            chunk_id,
            chunk_index: chunk.index as i64,
            size_bytes: chunk.data.len() as i64,
            sha256_hex: chunk.sha256_hex.clone(),
        });

        info!(
            "chunk stored: chunk_id={} index={} size={} replicas={}",
            chunk_id,
            chunk.index,
            chunk.data.len(),
            node_ids.len()
        );
    }

    // Complete the upload with the coordinator.
    coord_client
        .complete_upload(upload_id, file_id, chunk_manifests)
        .await
        .map_err(|e| crate::error::GatewayError::Grpc(e.to_string()))?;

    info!(
        "upload completed: file_id={} upload_id={} chunks={}",
        file_id, upload_id, chunk_count
    );

    Ok(Json(UploadResponse {
        file_id: file_id.to_string(),
        upload_id: upload_id.to_string(),
        chunk_size: chunk_size as i64,
        chunk_count,
        replication_factor: rf,
    }))
}

/// Store a chunk to all replica nodes concurrently.
async fn store_chunk_to_replicas(
    node_addrs: &std::collections::HashMap<uuid::Uuid, String>,
    node_ids: &[uuid::Uuid],
    chunk_id: &uuid::Uuid,
    data: &[u8],
    sha256_hex: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut handles = Vec::new();

    for node_id in node_ids {
        let addr = node_addrs
            .get(node_id)
            .ok_or_else(|| format!("no address for node {node_id}"))?;
        let addr = addr.clone();
        let chunk_id = *chunk_id;
        let data = data.to_vec();
        let sha256_hex = sha256_hex.to_string();

        handles.push(tokio::spawn(async move {
            store_chunk_to_node(&addr, chunk_id, data.clone(), sha256_hex.clone()).await
        }));
    }

    for handle in handles {
        handle.await??;
    }

    Ok(())
}

/// Store a single chunk to one storage node via streaming gRPC.
async fn store_chunk_to_node(
    grpc_addr: &str,
    chunk_id: uuid::Uuid,
    data: Vec<u8>,
    sha256_hex: String,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use proto::storage::chunk_service_client::ChunkServiceClient;
    use proto::storage::{ChunkReference, StoreChunkRequest};

    let mut client = ChunkServiceClient::connect(grpc_addr.to_string()).await?;

    let meta = StoreChunkRequest {
        payload: Some(proto::storage::store_chunk_request::Payload::Meta(
            ChunkReference {
                chunk_id: chunk_id.to_string(),
                file_id: String::new(),
                chunk_index: 0,
                size_bytes: data.len() as i64,
                sha256_hex: sha256_hex.clone(),
                replication_factor: 0,
            },
        )),
    };

    let stream = async_stream::stream! {
        yield meta;
        let frame_size = 64 * 1024;
        for chunk in data.chunks(frame_size) {
            yield StoreChunkRequest {
                payload: Some(proto::storage::store_chunk_request::Payload::Data(chunk.to_vec())),
            };
        }
    };

    let response = client.store_chunk(tonic::Request::new(stream)).await?;
    let response = response.into_inner();

    if !response.durable {
        return Err(format!("chunk {chunk_id} not durably stored on {grpc_addr}").into());
    }

    Ok(())
}

/// Wraps a `Stream<Result<Bytes, E>>` as `AsyncRead`.
struct StreamReader<S> {
    stream: std::pin::Pin<Box<S>>,
    buffer: std::collections::VecDeque<u8>,
    exhausted: bool,
}

impl<S> StreamReader<S>
where
    S: futures_util::stream::Stream<Item = Result<bytes::Bytes, std::io::Error>> + Unpin,
{
    fn new(stream: S) -> Self {
        Self {
            stream: Box::pin(stream),
            buffer: std::collections::VecDeque::new(),
            exhausted: false,
        }
    }
}

impl<S> tokio::io::AsyncRead for StreamReader<S>
where
    S: futures_util::stream::Stream<Item = Result<bytes::Bytes, std::io::Error>> + Unpin,
{
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        let this = self.get_mut();

        while !this.buffer.is_empty() && buf.remaining() > 0 {
            let byte = this.buffer.pop_front().unwrap();
            buf.put_slice(&[byte]);
        }

        if buf.remaining() == 0 {
            return std::task::Poll::Ready(Ok(()));
        }

        if this.exhausted {
            return std::task::Poll::Ready(Ok(()));
        }

        match this.stream.as_mut().poll_next(cx) {
            std::task::Poll::Ready(Some(Ok(bytes))) => {
                this.buffer.extend(bytes.iter().copied());
                cx.waker().wake_by_ref();
                std::task::Poll::Pending
            }
            std::task::Poll::Ready(Some(Err(e))) => {
                this.exhausted = true;
                std::task::Poll::Ready(Err(e))
            }
            std::task::Poll::Ready(None) => {
                this.exhausted = true;
                std::task::Poll::Ready(Ok(()))
            }
            std::task::Poll::Pending => std::task::Poll::Pending,
        }
    }
}
