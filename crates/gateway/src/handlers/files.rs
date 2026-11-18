//! File metadata, listing, and deletion handlers.
//! GET /api/v1/files
//! GET /api/v1/files/{file_id}
//! DELETE /api/v1/files/{file_id}

use axum::extract::{Path, Query, State};
use axum::response::Json;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::warn;

use crate::AppState;
use crate::error::GatewayError;

/// Map coordinator errors to gateway errors, preserving `NotFound` so clients
/// get a 404 instead of a generic 502.
fn map_coord_error(e: Box<dyn std::error::Error + Send + Sync>) -> GatewayError {
    GatewayError::coordinator(e)
}

#[derive(Serialize)]
pub struct FileInfoResponse {
    pub file_id: String,
    pub filename: String,
    pub total_size: i64,
    pub chunk_size: i64,
    pub chunk_count: i32,
    pub replication_factor: i32,
    pub status: String,
}

#[derive(serde::Deserialize)]
pub struct ListFilesQuery {
    pub limit: Option<i32>,
    pub cursor: Option<String>,
}

#[derive(Serialize)]
pub struct FileSummaryResponse {
    pub file_id: String,
    pub filename: String,
    pub total_size: i64,
    pub chunk_count: i32,
    pub replication_factor: i32,
    pub status: String,
    pub created_epoch_ms: i64,
}

#[derive(Serialize)]
pub struct FileListResponse {
    pub files: Vec<FileSummaryResponse>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

const DEFAULT_LIST_LIMIT: i32 = 50;
const MAX_LIST_LIMIT: i32 = 200;

/// List committed files newest-first with opaque keyset pagination.
pub async fn list_files(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ListFilesQuery>,
) -> Result<Json<FileListResponse>, crate::error::GatewayError> {
    let limit = query.limit.unwrap_or(DEFAULT_LIST_LIMIT);
    if !(1..=MAX_LIST_LIMIT).contains(&limit) {
        return Err(crate::error::GatewayError::InvalidArgument(format!(
            "limit must be between 1 and {MAX_LIST_LIMIT}"
        )));
    }

    let mut client = state.coord_client().await?;
    let page = client
        .list_files(limit, query.cursor.unwrap_or_default())
        .await
        .map_err(map_coord_error)?;

    Ok(Json(FileListResponse {
        files: page
            .files
            .into_iter()
            .map(|f| FileSummaryResponse {
                file_id: f.file_id.to_string(),
                filename: f.filename,
                total_size: f.total_size,
                chunk_count: f.chunk_count,
                replication_factor: f.replication_factor,
                status: f.status,
                created_epoch_ms: f.created_epoch_ms,
            })
            .collect(),
        next_cursor: page.next_cursor,
    }))
}

pub async fn get_file(
    State(state): State<Arc<AppState>>,
    Path(file_id): Path<String>,
) -> Result<Json<FileInfoResponse>, crate::error::GatewayError> {
    let file_id = uuid::Uuid::parse_str(&file_id)
        .map_err(|e| crate::error::GatewayError::InvalidArgument(e.to_string()))?;

    let mut client = state.coord_client().await?;
    let manifest = client
        .get_manifest(file_id)
        .await
        .map_err(map_coord_error)?;

    Ok(Json(FileInfoResponse {
        file_id: manifest.file_id.to_string(),
        filename: manifest.filename,
        total_size: manifest.total_size,
        chunk_size: manifest.chunk_size,
        chunk_count: manifest.chunk_count,
        replication_factor: manifest.replication_factor,
        status: "committed".to_string(),
    }))
}

pub async fn delete_file(
    State(state): State<Arc<AppState>>,
    Path(file_id): Path<String>,
) -> Result<Json<serde_json::Value>, crate::error::GatewayError> {
    let file_id = uuid::Uuid::parse_str(&file_id)
        .map_err(|e| crate::error::GatewayError::InvalidArgument(e.to_string()))?;

    // Remove metadata first. If byte deletion below fails, the chunks become
    // unreferenced rather than leaving metadata pointing at missing data.
    let mut client = state.coord_client().await?;
    let chunks = client.delete_file(file_id).await.map_err(map_coord_error)?;

    let nodes = client.list_nodes().await.map_err(map_coord_error)?;
    drop(client);

    let node_addrs: HashMap<uuid::Uuid, String> = nodes
        .into_iter()
        .map(|n| (n.node_id, n.grpc_addr))
        .collect();

    let mut replicas_deleted = 0u64;
    let mut replicas_failed = 0u64;
    for (chunk_id, node_ids) in &chunks {
        for node_id in node_ids {
            let Some(addr) = node_addrs.get(node_id) else {
                continue;
            };
            match delete_chunk_from_node(addr, chunk_id).await {
                Ok(true) => replicas_deleted += 1,
                Ok(false) => {}
                Err(e) => {
                    replicas_failed += 1;
                    warn!("failed to delete chunk {chunk_id} from node {node_id}: {e}");
                }
            }
        }
    }

    Ok(Json(serde_json::json!({
        "status": "deleted",
        "file_id": file_id.to_string(),
        "chunks": chunks.len(),
        "chunk_replicas_deleted": replicas_deleted,
        "chunk_replicas_failed": replicas_failed,
    })))
}

/// Delete a chunk's bytes from one storage node via gRPC.
async fn delete_chunk_from_node(
    grpc_addr: &str,
    chunk_id: &uuid::Uuid,
) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
    use proto::storage::DeleteChunkRequest;
    use proto::storage::chunk_service_client::ChunkServiceClient;

    let mut client = ChunkServiceClient::connect(crate::grpc_endpoint(grpc_addr)).await?;
    let response = client
        .delete_chunk(tonic::Request::new(DeleteChunkRequest {
            chunk_id: chunk_id.to_string(),
        }))
        .await?;
    Ok(response.into_inner().deleted)
}
