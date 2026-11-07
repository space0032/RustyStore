//! File metadata and deletion handlers.
//! GET /api/v1/files/{file_id}
//! DELETE /api/v1/files/{file_id}

use axum::extract::{Path, State};
use axum::response::Json;
use serde::Serialize;
use std::sync::Arc;

use crate::AppState;

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
        .map_err(|e| crate::error::GatewayError::Grpc(e.to_string()))?;

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
    State(_state): State<Arc<AppState>>,
    Path(_file_id): Path<String>,
) -> Result<Json<serde_json::Value>, crate::error::GatewayError> {
    // TODO: implement delete with reference counting
    Ok(Json(serde_json::json!({
        "status": "deleted",
        "message": "delete not yet implemented"
    })))
}
