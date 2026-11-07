//! API route definitions.

use axum::{
    Router,
    routing::{get, post},
};
use std::sync::Arc;

use crate::AppState;
use crate::handlers::{cluster, download, files, health, upload};

pub fn routes(state: Arc<AppState>) -> Router {
    Router::new()
        // Health
        .route("/api/v1/health", get(health::health))
        // File operations
        .route("/api/v1/files", post(upload::upload_file))
        .route(
            "/api/v1/files/:file_id",
            get(files::get_file).delete(files::delete_file),
        )
        .route(
            "/api/v1/files/:file_id/download",
            get(download::download_file),
        )
        // Cluster info
        .route("/api/v1/cluster/nodes", get(cluster::list_nodes))
        .route("/api/v1/cluster/stats", get(cluster::cluster_stats))
        .with_state(state)
}
