//! Coordinator client for the gateway.
//!
//! Wraps the tonic-generated ClusterServiceClient to provide higher-level
//! methods for upload sessions, chunk placement, manifests, and cluster info.

use proto::storage::{
    ChunkManifest, CompleteUploadRequest, CreateUploadSessionRequest, DeleteFileRequest,
    GetChunkPlanRequest, GetManifestRequest, GetStatsRequest, ListFilesRequest, ListNodesRequest,
    RateLimitRequest, cluster_service_client::ClusterServiceClient,
};
use std::future::Future;
use std::io;
use std::pin::Pin;
use tonic::Code;
use tonic::transport::Channel;
use uuid::Uuid;

/// Whether a failed RPC may succeed against another coordinator.
fn retryable(status: &tonic::Status) -> bool {
    matches!(
        status.code(),
        Code::Unavailable
            | Code::DeadlineExceeded
            | Code::Cancelled
            | Code::Unknown
            | Code::Internal
            | Code::ResourceExhausted
    )
}

fn boxed_error(message: impl Into<String>) -> Box<dyn std::error::Error + Send + Sync> {
    Box::new(io::Error::other(message.into()))
}

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
///
/// Holds one or more coordinator endpoints. Retryable RPC failures rotate to
/// the next endpoint, so a gateway keeps working when a coordinator restarts
/// or fails over. A single address behaves like the original client.
#[derive(Clone)]
pub struct CoordinatorClient {
    addrs: Vec<String>,
    current: usize,
    inner: ClusterServiceClient<Channel>,
}

impl CoordinatorClient {
    pub async fn connect(addr: &str) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        Self::connect_many(&[addr.to_string()]).await
    }

    pub async fn connect_many(
        addrs: &[String],
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        if addrs.is_empty() {
            return Err(boxed_error("no coordinator addresses configured"));
        }
        let mut last_error = None;
        for (current, addr) in addrs.iter().enumerate() {
            match Self::connect_at(addr).await {
                Ok(inner) => {
                    return Ok(Self {
                        addrs: addrs.to_vec(),
                        current,
                        inner,
                    });
                }
                Err(e) => last_error = Some(e.to_string()),
            }
        }
        Err(boxed_error(last_error.unwrap_or_else(|| {
            "no coordinator addresses configured".to_string()
        })))
    }

    async fn connect_at(
        addr: &str,
    ) -> Result<ClusterServiceClient<Channel>, Box<dyn std::error::Error + Send + Sync>> {
        let channel = common::tls::connect(addr).await?;
        Ok(ClusterServiceClient::new(channel))
    }

    /// Reconnect to the next configured coordinator.
    async fn rotate(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if self.addrs.len() <= 1 {
            return Ok(());
        }
        let mut last_error = None;
        for offset in 1..=self.addrs.len() {
            let index = (self.current + offset) % self.addrs.len();
            match Self::connect_at(&self.addrs[index]).await {
                Ok(inner) => {
                    self.inner = inner;
                    self.current = index;
                    return Ok(());
                }
                Err(e) => last_error = Some(e.to_string()),
            }
        }
        Err(boxed_error(last_error.unwrap_or_else(|| {
            "no coordinator addresses configured".to_string()
        })))
    }

    async fn with_retry<T, F>(
        &mut self,
        mut operation: F,
    ) -> Result<T, Box<dyn std::error::Error + Send + Sync>>
    where
        F: FnMut(
            &mut ClusterServiceClient<Channel>,
        ) -> Pin<Box<dyn Future<Output = Result<T, tonic::Status>> + Send + '_>>,
    {
        let attempts = self.addrs.len().max(1);
        let mut last_error: Option<Box<dyn std::error::Error + Send + Sync>> = None;
        for _ in 0..attempts {
            match operation(&mut self.inner).await {
                Ok(value) => return Ok(value),
                Err(status) if retryable(&status) => {
                    last_error = Some(Box::new(status));
                    if let Err(e) = self.rotate().await {
                        last_error = Some(e);
                    }
                }
                Err(status) => return Err(Box::new(status)),
            }
        }
        Err(last_error.unwrap_or_else(|| boxed_error("coordinator unavailable")))
    }

    /// Create an upload session. Returns the coordinator-issued
    /// `(upload_id, file_id, chunk_size, replication_factor)`; the coordinator
    /// owns chunking and placement settings.
    pub async fn create_upload_session(
        &mut self,
        filename: &str,
        total_size: i64,
    ) -> Result<(Uuid, Uuid, i64, i32), Box<dyn std::error::Error + Send + Sync>> {
        let resp = self
            .with_retry(|inner| {
                let req = request_with_id(CreateUploadSessionRequest {
                    filename: filename.to_string(),
                    total_size,
                    chunk_size: 0,
                    replication_factor: 0,
                });
                Box::pin(async move {
                    inner
                        .create_upload_session(req)
                        .await
                        .map(|response| response.into_inner())
                })
            })
            .await?;
        let upload_id = Uuid::parse_str(&resp.upload_id)?;
        let file_id = Uuid::parse_str(&resp.file_id)?;
        Ok((upload_id, file_id, resp.chunk_size, resp.replication_factor))
    }

    pub async fn get_chunk_plan(
        &mut self,
        upload_id: Uuid,
        chunk_index: i64,
    ) -> Result<(String, Vec<Uuid>), Box<dyn std::error::Error + Send + Sync>> {
        let inner = self
            .with_retry(|inner| {
                let req = request_with_id(GetChunkPlanRequest {
                    upload_id: upload_id.to_string(),
                    chunk_index,
                });
                Box::pin(async move {
                    inner
                        .get_chunk_plan(req)
                        .await
                        .map(|response| response.into_inner())
                })
            })
            .await?;
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
        self.with_retry(|inner| {
            let req = request_with_id(CompleteUploadRequest {
                upload_id: upload_id.to_string(),
                file_id: file_id.to_string(),
                chunks: chunks
                    .clone()
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
            Box::pin(async move { inner.complete_upload(req).await.map(|_| ()) })
        })
        .await?;
        Ok(file_id)
    }

    pub async fn get_manifest(
        &mut self,
        file_id: Uuid,
    ) -> Result<crate::types::Manifest, Box<dyn std::error::Error + Send + Sync>> {
        let inner = self
            .with_retry(|inner| {
                let req = request_with_id(GetManifestRequest {
                    file_id: file_id.to_string(),
                });
                Box::pin(async move {
                    inner
                        .get_manifest(req)
                        .await
                        .map(|response| response.into_inner())
                })
            })
            .await?;
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
        let resp = self
            .with_retry(|inner| {
                let req = request_with_id(DeleteFileRequest {
                    file_id: file_id.to_string(),
                });
                Box::pin(async move {
                    inner
                        .delete_file(req)
                        .await
                        .map(|response| response.into_inner())
                })
            })
            .await?;
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
        let resp = self
            .with_retry(|inner| {
                let req = request_with_id(ListFilesRequest {
                    limit,
                    cursor: cursor.clone(),
                });
                Box::pin(async move {
                    inner
                        .list_files(req)
                        .await
                        .map(|response| response.into_inner())
                })
            })
            .await?;
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
        let inner = self
            .with_retry(|inner| {
                let req = request_with_id(ListNodesRequest {});
                Box::pin(async move {
                    inner
                        .list_nodes(req)
                        .await
                        .map(|response| response.into_inner())
                })
            })
            .await?;
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

    /// Consume one token from the cluster-wide rate limiter for `key`.
    pub async fn check_rate_limit(
        &mut self,
        key: &str,
        rps: u32,
        burst: u32,
    ) -> Result<(bool, u64), Box<dyn std::error::Error + Send + Sync>> {
        let resp = self
            .with_retry(|inner| {
                let req = request_with_id(RateLimitRequest {
                    key: key.to_string(),
                    rps: rps.min(i32::MAX as u32) as i32,
                    burst: burst.min(i32::MAX as u32) as i32,
                });
                Box::pin(async move {
                    inner
                        .check_rate_limit(req)
                        .await
                        .map(|response| response.into_inner())
                })
            })
            .await?;
        Ok((resp.allowed, resp.retry_after_ms.max(0) as u64))
    }

    pub async fn get_stats(
        &mut self,
    ) -> Result<crate::types::ClusterStats, Box<dyn std::error::Error + Send + Sync>> {
        let inner = self
            .with_retry(|inner| {
                let req = request_with_id(GetStatsRequest {});
                Box::pin(async move {
                    inner
                        .get_stats(req)
                        .await
                        .map(|response| response.into_inner())
                })
            })
            .await?;
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
    use super::{CoordinatorClient, request_with_id, retryable};

    #[test]
    fn only_transient_statuses_are_retryable() {
        assert!(retryable(&tonic::Status::unavailable("down")));
        assert!(retryable(&tonic::Status::deadline_exceeded("slow")));
        assert!(!retryable(&tonic::Status::not_found("missing")));
        assert!(!retryable(&tonic::Status::invalid_argument("bad")));
    }

    #[tokio::test]
    async fn connect_many_rejects_an_empty_address_list() {
        assert!(CoordinatorClient::connect_many(&[]).await.is_err());
    }

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
