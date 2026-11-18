//! File download handler.
//! GET /api/v1/files/{file_id}/download
//!
//! Streaming download: fetches the manifest, retrieves chunks from replicas
//! in order, verifies SHA-256, and streams the reconstructed file to the
//! client. Never reassembles the entire file in memory.

use axum::extract::{Path, State};
use axum::response::Response;
use futures_util::StreamExt;
use std::sync::Arc;
use tracing::{info, warn};

use crate::AppState;

pub async fn download_file(
    State(state): State<Arc<AppState>>,
    Path(file_id): Path<String>,
) -> Result<Response, crate::error::GatewayError> {
    let file_id = uuid::Uuid::parse_str(&file_id)
        .map_err(|e| crate::error::GatewayError::InvalidArgument(e.to_string()))?;

    // Get manifest from coordinator.
    let mut coord_client = state.coord_client().await?;
    let manifest = coord_client
        .get_manifest(file_id)
        .await
        .map_err(crate::error::GatewayError::coordinator)?;

    info!(
        "download started: file_id={} filename={} chunks={} total_size={}",
        manifest.file_id, manifest.filename, manifest.chunk_count, manifest.total_size
    );

    let (tx, rx) = tokio::sync::mpsc::channel::<Result<bytes::Bytes, std::io::Error>>(16);

    let state_clone = state.clone();
    let manifest_clone = manifest.clone();
    // Spawned tasks do not inherit task-locals: carry the request id across.
    let request_id = common::request_id::current();
    tokio::spawn(common::request_id::scope_opt(request_id, async move {
        if let Err(e) = fetch_and_stream_chunks(state_clone, manifest_clone, tx).await {
            warn!("download stream error: {e}");
        }
    }));

    let body = axum::body::Body::from_stream(
        tokio_stream::wrappers::ReceiverStream::new(rx).map(|r| r.map_err(std::io::Error::other)),
    );
    let response = Response::builder()
        .header("content-type", "application/octet-stream")
        .header("content-length", manifest.total_size.to_string())
        .header("x-file-id", manifest.file_id.to_string())
        .body(body)
        .map_err(|e| crate::error::GatewayError::Internal(e.to_string()))?;

    Ok(response)
}

/// Fetch chunks in order and send them down the channel.
async fn fetch_and_stream_chunks(
    state: Arc<AppState>,
    manifest: coordinator::types::Manifest,
    tx: tokio::sync::mpsc::Sender<Result<bytes::Bytes, std::io::Error>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Build node_id -> grpc_addr map.
    let mut coord_client = state.coord_client().await?;
    let nodes = coord_client
        .list_nodes()
        .await
        .map_err(|e| Box::<dyn std::error::Error + Send + Sync>::from(e.to_string()))?;
    let node_addrs: std::collections::HashMap<uuid::Uuid, String> = nodes
        .into_iter()
        .map(|n| (n.node_id, n.grpc_addr))
        .collect();
    drop(coord_client);

    for chunk_entry in &manifest.chunks {
        let mut last_err: Option<String> = None;

        // Try replicas in order until one succeeds.
        for node_id in &chunk_entry.node_ids {
            let addr = match node_addrs.get(node_id) {
                Some(a) => a.clone(),
                None => continue,
            };

            match retrieve_chunk_from_node(&addr, &chunk_entry.chunk_id, &chunk_entry.sha256_hex)
                .await
            {
                Ok(data) => {
                    observability::DOWNLOADED_BYTES.inc_by(data.len() as u64);
                    if tx.send(Ok(bytes::Bytes::from(data))).await.is_err() {
                        return Ok(()); // receiver dropped
                    }
                    last_err = None;
                    break;
                }
                Err(e) => {
                    last_err = Some(format!("node {}: {e}", node_id));
                    warn!("replica failed for chunk {}: {e}", chunk_entry.chunk_id);
                }
            }
        }

        if let Some(err) = last_err {
            let _ = tx.send(Err(std::io::Error::other(err))).await;
            return Err(
                format!("chunk {} unavailable on all replicas", chunk_entry.chunk_id).into(),
            );
        }
    }

    Ok(())
}

/// Retrieve a single chunk from a storage node via streaming gRPC.
async fn retrieve_chunk_from_node(
    grpc_addr: &str,
    chunk_id: &uuid::Uuid,
    expected_sha256: &str,
) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
    use proto::storage::RetrieveChunkRequest;
    use proto::storage::chunk_service_client::ChunkServiceClient;

    let mut client = ChunkServiceClient::connect(crate::grpc_endpoint(grpc_addr)).await?;

    let request = RetrieveChunkRequest {
        chunk_id: chunk_id.to_string(),
        expected_sha256: expected_sha256.to_string(),
    };

    let mut stream = client
        .retrieve_chunk(crate::grpc::request_with_id(request))
        .await?
        .into_inner();

    let mut data = Vec::new();
    let mut meta_received = false;

    while let Some(frame) = stream.message().await? {
        match frame.payload {
            Some(proto::storage::retrieve_chunk_response::Payload::Meta(_m)) => {
                meta_received = true;
            }
            Some(proto::storage::retrieve_chunk_response::Payload::Data(chunk_bytes)) => {
                data.extend_from_slice(&chunk_bytes);
            }
            None => {}
        }
    }

    if !meta_received {
        return Err("no metadata received from storage node".into());
    }

    // Verify SHA-256.
    let actual = sha256_hex(data.as_slice());
    if actual != expected_sha256 {
        return Err(format!(
            "chunk {} integrity mismatch: expected {} got {}",
            chunk_id, expected_sha256, actual
        )
        .into());
    }

    Ok(data)
}

fn sha256_hex(data: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(data);
    hex::encode(hasher.finalize())
}
