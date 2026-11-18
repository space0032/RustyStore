//! Coordinator client for the gateway.
//!
//! Wraps the tonic-generated ClusterServiceClient to provide higher-level
//! methods for upload sessions, chunk placement, manifests, and cluster info.

use proto::storage::{
    ChunkManifest, CompleteUploadRequest, CreateUploadSessionRequest, DeleteFileRequest,
    GetChunkPlanRequest, GetManifestRequest, GetStatsRequest, ListFilesRequest, ListNodesRequest,
    cluster_service_client::ClusterServiceClient,
};
use tonic::transport::Channel;
use uuid::Uuid;

/// Wrap a message in a `tonic::Request`, attaching the request id scoped in
/// `common::request_id` as metadata when one is present.
fn request_with_id<T>(message: T) -> tonic::Request<T> {
    let mut request = tonic::Request::new(message);
    if let Some(id) = common::request_id::current()
        && let Ok(value) = id.parse::<tonic::metadata::MetadataValue<tonic::metadata::Ascii>>()
    {
        request
            .metadata_mut()
            .insert(common::request_id::HEADER, value);
    }
    request
}

/// High-level client for talking to the Coordinator.
#[derive(Clone)]
pub struct CoordinatorClient {
    inner: ClusterServiceClient<Channel>,
}

impl CoordinatorClient {
    pub async fn connect(addr: &str) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let channel = Channel::from_shared(addr.to_string())?.connect().await?;
        let inner = ClusterServiceClient::new(channel);
        Ok(Self { inner })
    }

    /// Create an upload session. Returns the coordinator-issued
    /// `(upload_id, file_id, chunk_size, replication_factor)`; the coordinator
    /// owns chunking and placement settings.
    pub async fn create_upload_session(
        &mut self,
        filename: &str,
        total_size: i64,
    ) -> Result<(Uuid, Uuid, i64, i32), Box<dyn std::error::Error + Send + Sync>> {
        let req = request_with_id(CreateUploadSessionRequest {
            filename: filename.to_string(),
            total_size,
            chunk_size: 0,
            replication_factor: 0,
        });
        let resp = self.inner.create_upload_session(req).await?.into_inner();
        let upload_id = Uuid::parse_str(&resp.upload_id)?;
        let file_id = Uuid::parse_str(&resp.file_id)?;
        Ok((upload_id, file_id, resp.chunk_size, resp.replication_factor))
    }

    pub async fn get_chunk_plan(
        &mut self,
        upload_id: Uuid,
        chunk_index: i64,
    ) -> Result<(String, Vec<Uuid>), Box<dyn std::error::Error + Send + Sync>> {
        let req = request_with_id(GetChunkPlanRequest {
            upload_id: upload_id.to_string(),
            chunk_index,
        });
        let resp = self.inner.get_chunk_plan(req).await?;
        let inner = resp.into_inner();
        let node_ids: Vec<Uuid> = inner
            .node_ids
            .iter()
            .filter_map(|s| Uuid::parse_str(s).ok())
            .collect();
        Ok((inner.chunk_id, node_ids))
    }

    pub async fn complete_upload(
        &mut self,
        upload_id: Uuid,
        file_id: Uuid,
        chunks: Vec<crate::types::ChunkManifestEntry>,
    ) -> Result<Uuid, Box<dyn std::error::Error + Send + Sync>> {
        let req = request_with_id(CompleteUploadRequest {
            upload_id: upload_id.to_string(),
            file_id: file_id.to_string(),
            chunks: chunks
                .into_iter()
                .map(|c| ChunkManifest {
                    chunk_id: c.chunk_id.to_string(),
                    chunk_index: c.chunk_index,
                    size_bytes: c.size_bytes,
                    sha256_hex: c.sha256_hex,
                    node_ids: c.node_ids.iter().map(ToString::to_string).collect(),
                })
                .collect(),
        });
        let _resp = self.inner.complete_upload(req).await?;
        Ok(file_id)
    }

    pub async fn get_manifest(
        &mut self,
        file_id: Uuid,
    ) -> Result<crate::types::Manifest, Box<dyn std::error::Error + Send + Sync>> {
        let req = request_with_id(GetManifestRequest {
            file_id: file_id.to_string(),
        });
        let resp = self.inner.get_manifest(req).await?;
        let inner = resp.into_inner();
        Ok(crate::types::Manifest {
            file_id: Uuid::parse_str(&inner.file_id).unwrap_or(file_id),
            filename: inner.filename,
            total_size: inner.total_size,
            chunk_size: inner.chunk_size,
            chunk_count: inner.chunk_count,
            replication_factor: inner.replication_factor,
            chunks: inner
                .chunks
                .into_iter()
                .map(|c| crate::types::ChunkLocationEntry {
                    chunk_id: Uuid::parse_str(&c.chunk_id).unwrap_or(Uuid::nil()),
                    chunk_index: c.chunk_index,
                    size_bytes: c.size_bytes,
                    sha256_hex: c.sha256_hex,
                    node_ids: c
                        .node_ids
                        .iter()
                        .filter_map(|s| Uuid::parse_str(s).ok())
                        .collect(),
                })
                .collect(),
        })
    }

    /// Delete a file's metadata and return the chunk replicas that should be
    /// removed from storage nodes. Each entry is `(chunk_id, node_ids)`.
    pub async fn delete_file(
        &mut self,
        file_id: Uuid,
    ) -> Result<Vec<(Uuid, Vec<Uuid>)>, Box<dyn std::error::Error + Send + Sync>> {
        let req = request_with_id(DeleteFileRequest {
            file_id: file_id.to_string(),
        });
        let resp = self.inner.delete_file(req).await?.into_inner();
        Ok(resp
            .chunks
            .into_iter()
            .map(|c| {
                (
                    Uuid::parse_str(&c.chunk_id).unwrap_or(Uuid::nil()),
                    c.node_ids
                        .iter()
                        .filter_map(|s| Uuid::parse_str(s).ok())
                        .collect(),
                )
            })
            .collect())
    }

    /// List committed files newest-first. Pass an empty cursor for the first
    /// page and the returned `next_cursor` for subsequent pages.
    pub async fn list_files(
        &mut self,
        limit: i32,
        cursor: String,
    ) -> Result<crate::types::FileList, Box<dyn std::error::Error + Send + Sync>> {
        let req = request_with_id(ListFilesRequest { limit, cursor });
        let resp = self.inner.list_files(req).await?.into_inner();
        Ok(crate::types::FileList {
            files: resp
                .files
                .into_iter()
                .map(|f| crate::types::FileSummaryEntry {
                    file_id: Uuid::parse_str(&f.file_id).unwrap_or(Uuid::nil()),
                    filename: f.filename,
                    total_size: f.total_size,
                    chunk_count: i32::try_from(f.chunk_count).unwrap_or(i32::MAX),
                    replication_factor: f.replication_factor,
                    status: f.status,
                    created_epoch_ms: f.created_epoch_ms,
                })
                .collect(),
            next_cursor: (!resp.next_cursor.is_empty()).then_some(resp.next_cursor),
        })
    }

    pub async fn list_nodes(
        &mut self,
    ) -> Result<Vec<crate::types::NodeInfoEntry>, Box<dyn std::error::Error + Send + Sync>> {
        let req = request_with_id(ListNodesRequest {});
        let resp = self.inner.list_nodes(req).await?;
        let inner = resp.into_inner();
        Ok(inner
            .nodes
            .into_iter()
            .map(|n| crate::types::NodeInfoEntry {
                node_id: Uuid::parse_str(&n.node_id).unwrap_or(Uuid::nil()),
                addr: n.addr,
                grpc_addr: n.grpc_addr,
                healthy: n.healthy,
                available_bytes: n.available_bytes,
                capacity_bytes: n.capacity_bytes,
            })
            .collect())
    }

    pub async fn get_stats(
        &mut self,
    ) -> Result<crate::types::ClusterStats, Box<dyn std::error::Error + Send + Sync>> {
        let req = request_with_id(GetStatsRequest {});
        let resp = self.inner.get_stats(req).await?;
        let inner = resp.into_inner();
        Ok(crate::types::ClusterStats {
            total_nodes: inner.total_nodes,
            healthy_nodes: inner.healthy_nodes,
            total_capacity_bytes: inner.total_capacity_bytes,
            total_available_bytes: inner.total_available_bytes,
            total_files: inner.total_files,
            total_chunks: inner.total_chunks,
            under_replicated_chunks: inner.under_replicated_chunks,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::request_with_id;

    #[tokio::test]
    async fn attaches_current_request_id() {
        let request = common::request_id::scope("trace-1", async { request_with_id(()) }).await;
        let value = request
            .metadata()
            .get(common::request_id::HEADER)
            .expect("request id metadata");
        assert_eq!(value.to_str().unwrap(), "trace-1");
    }

    #[tokio::test]
    async fn omits_header_without_scope() {
        let request = request_with_id(());
        assert!(request.metadata().get(common::request_id::HEADER).is_none());
    }
}
