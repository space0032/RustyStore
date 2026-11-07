//! Cluster info handlers.
//! GET /api/v1/cluster/nodes
//! GET /api/v1/cluster/stats

use axum::extract::State;
use axum::response::Json;
use serde::Serialize;
use std::sync::Arc;

use crate::AppState;

#[derive(Serialize)]
pub struct NodeInfoResponse {
    pub node_id: String,
    pub addr: String,
    pub grpc_addr: String,
    pub healthy: bool,
    pub available_bytes: i64,
    pub capacity_bytes: i64,
}

#[derive(Serialize)]
pub struct ClusterStatsResponse {
    pub total_nodes: i32,
    pub healthy_nodes: i32,
    pub total_capacity_bytes: i64,
    pub total_available_bytes: i64,
    pub total_files: i64,
    pub total_chunks: i64,
    pub under_replicated_chunks: i64,
}

pub async fn list_nodes(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<NodeInfoResponse>>, crate::error::GatewayError> {
    let mut client = state.coord_client().await?;
    let nodes = client
        .list_nodes()
        .await
        .map_err(|e| crate::error::GatewayError::Grpc(e.to_string()))?;

    Ok(Json(
        nodes
            .into_iter()
            .map(|n| NodeInfoResponse {
                node_id: n.node_id.to_string(),
                addr: n.addr,
                grpc_addr: n.grpc_addr,
                healthy: n.healthy,
                available_bytes: n.available_bytes,
                capacity_bytes: n.capacity_bytes,
            })
            .collect(),
    ))
}

pub async fn cluster_stats(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ClusterStatsResponse>, crate::error::GatewayError> {
    let mut client = state.coord_client().await?;
    let stats = client
        .get_stats()
        .await
        .map_err(|e| crate::error::GatewayError::Grpc(e.to_string()))?;

    Ok(Json(ClusterStatsResponse {
        total_nodes: stats.total_nodes,
        healthy_nodes: stats.healthy_nodes,
        total_capacity_bytes: stats.total_capacity_bytes,
        total_available_bytes: stats.total_available_bytes,
        total_files: stats.total_files,
        total_chunks: stats.total_chunks,
        under_replicated_chunks: stats.under_replicated_chunks,
    }))
}
