//! API route definitions.

use axum::{Router, routing::get};
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};

use crate::AppState;
use crate::handlers::{cluster, download, files, health, upload};

pub fn routes(state: Arc<AppState>) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any)
        .expose_headers([
            http::header::HeaderName::from_static("x-file-id"),
            http::header::HeaderName::from_static("x-request-id"),
            http::header::CONTENT_LENGTH,
            http::header::CONTENT_TYPE,
        ]);

    Router::new()
        // Health, readiness & metrics
        .route("/api/v1/health", get(health::health))
        .route("/api/v1/ready", get(health::ready))
        .route("/metrics", get(health::metrics))
        // File operations
        .route(
            "/api/v1/files",
            get(files::list_files).post(upload::upload_file),
        )
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
        .layer(cors)
        .with_state(state)
}
