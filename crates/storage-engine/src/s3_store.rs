//! S3-compatible chunk store (MinIO, AWS S3, etc.).
//!
//! Implements the `ChunkStore` trait backed by an S3-compatible object store.
//! Chunks are stored as S3 objects with the key = chunk_id.
//!
//! Enable with the `s3` feature flag:
//!   cargo build --features storage-engine/s3

use crate::chunk_store::{
    CapacityInfo, ChunkEntry, ChunkMeta, ChunkStore, StoreResult, StoredChunkInfo,
};
use async_trait::async_trait;
use bytes::Bytes;
use common::error::RustyStoreError;
use sha2::{Digest, Sha256};
use std::path::Path;
use std::sync::Arc;

/// S3-compatible chunk store.
#[derive(Clone)]
pub struct S3ChunkStore {
    client: Arc<aws_sdk_s3::Client>,
    bucket: String,
    base_prefix: String,
}

impl S3ChunkStore {
    /// Create a new S3 chunk store.
    #[allow(clippy::too_many_arguments)]
    pub async fn new(
        endpoint: &str,
        bucket: &str,
        base_prefix: &str,
        access_key: Option<&str>,
        secret_key: Option<&str>,
        region: Option<&str>,
    ) -> StoreResult<Self> {
        use aws_config::BehaviorVersion;
        use aws_config::meta::region::RegionProviderChain;
        use aws_sdk_s3::config::Region;

        let region_provider =
            RegionProviderChain::first_try(region.map(|r| Region::new(r.to_owned())))
                .or_default_provider()
                .or_else(Region::new("us-east-1"));

        let sdk_config = aws_config::defaults(BehaviorVersion::latest())
            .region(region_provider)
            .endpoint_url(endpoint)
            .load()
            .await;

        let mut config_builder = aws_sdk_s3::config::Builder::from(&sdk_config);

        if let (Some(ak), Some(sk)) = (access_key, secret_key) {
            use aws_sdk_s3::config::Credentials;
            let credentials = Credentials::new(ak, sk, None, None, "manual");
            config_builder = config_builder.credentials_provider(credentials);
        }

        let config = config_builder.build();
        let client = Arc::new(aws_sdk_s3::Client::from_conf(config));

        Ok(Self {
            client,
            bucket: bucket.to_string(),
            base_prefix: base_prefix.to_string(),
        })
    }

    fn object_key(&self, chunk_id: &str) -> String {
        if self.base_prefix.is_empty() {
            chunk_id.to_string()
        } else {
            format!("{}/{}", self.base_prefix, chunk_id)
        }
    }

    /// Compute SHA-256 hex of a byte slice.
    fn sha256_hex(data: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(data);
        hex::encode(hasher.finalize())
    }
}

#[async_trait]
impl ChunkStore for S3ChunkStore {
    async fn store_chunk(
        &self,
        chunk_id: &str,
        data: &[u8],
        expected_sha256: &str,
    ) -> StoreResult<()> {
        let actual = Self::sha256_hex(data);
        if actual != expected_sha256 {
            return Err(RustyStoreError::IntegrityMismatch {
                expected: expected_sha256.to_string(),
                actual,
            });
        }

        let key = self.object_key(chunk_id);
        let body = Bytes::copy_from_slice(data);

        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(&key)
            .body(body.into())
            .send()
            .await
            .map_err(|e| RustyStoreError::Internal(e.to_string()))?;

        Ok(())
    }

    async fn retrieve_chunk(&self, chunk_id: &str) -> StoreResult<ChunkEntry> {
        let key = self.object_key(chunk_id);

        let resp = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(&key)
            .send()
            .await
            .map_err(|e| {
                if e.to_string().contains("NoSuchKey") || e.to_string().contains("NotFound") {
                    RustyStoreError::NotFound(format!("chunk {chunk_id} not found"))
                } else {
                    RustyStoreError::Internal(e.to_string())
                }
            })?;

        let data = resp
            .body
            .collect()
            .await
            .map_err(|e| RustyStoreError::Internal(e.to_string()))?
            .into_bytes()
            .to_vec();

        let actual = Self::sha256_hex(&data);

        Ok(ChunkEntry {
            chunk_id: chunk_id.to_string(),
            sha256_hex: actual,
            size_bytes: data.len() as u64,
            data,
        })
    }

    async fn open_chunk_reader(
        &self,
        chunk_id: &str,
    ) -> StoreResult<(ChunkMeta, Box<dyn tokio::io::AsyncRead + Unpin + Send>)> {
        // S3 doesn't support efficient random access reads without downloading
        // the full object. Download and return as a cursor.
        let entry = self.retrieve_chunk(chunk_id).await?;
        let meta = ChunkMeta {
            chunk_id: entry.chunk_id.clone(),
            sha256_hex: entry.sha256_hex.clone(),
            size_bytes: entry.size_bytes,
        };
        Ok((meta, Box::new(std::io::Cursor::new(entry.data))))
    }

    async fn verify_chunk(&self, chunk_id: &str, expected_sha256: &str) -> StoreResult<()> {
        // Download and verify SHA-256
        let entry = self.retrieve_chunk(chunk_id).await?;
        if entry.sha256_hex != expected_sha256 {
            return Err(RustyStoreError::IntegrityMismatch {
                expected: expected_sha256.to_string(),
                actual: entry.sha256_hex,
            });
        }
        Ok(())
    }

    async fn delete_chunk(&self, chunk_id: &str) -> StoreResult<bool> {
        let key = self.object_key(chunk_id);

        // Check if object exists first
        let head_result = self
            .client
            .head_object()
            .bucket(&self.bucket)
            .key(&key)
            .send()
            .await;

        match head_result {
            Ok(_) => {
                // Object exists, delete it
                self.client
                    .delete_object()
                    .bucket(&self.bucket)
                    .key(&key)
                    .send()
                    .await
                    .map_err(|e| RustyStoreError::Internal(e.to_string()))?;
                Ok(true)
            }
            Err(_) => {
                // Object doesn't exist
                Ok(false)
            }
        }
    }

    async fn list_chunks(&self) -> StoreResult<Vec<StoredChunkInfo>> {
        let prefix = if self.base_prefix.is_empty() {
            String::new()
        } else {
            format!("{}/", self.base_prefix)
        };

        let mut chunks = Vec::new();
        let mut continuation: Option<String> = None;
        loop {
            let mut request = self.client.list_objects_v2().bucket(&self.bucket);
            if !prefix.is_empty() {
                request = request.prefix(&prefix);
            }
            if let Some(token) = continuation.take() {
                request = request.continuation_token(token);
            }

            let response = request
                .send()
                .await
                .map_err(|e| RustyStoreError::Internal(e.to_string()))?;

            for object in response.contents() {
                let Some(key) = object.key() else { continue };
                let Some(chunk_id) = key.strip_prefix(&prefix) else {
                    continue;
                };
                if chunk_id.is_empty() || chunk_id.contains('/') {
                    continue;
                }
                let modified_epoch_ms = object
                    .last_modified()
                    .map(|t| t.secs() * 1000 + i64::from(t.subsec_nanos()) / 1_000_000)
                    .unwrap_or(0);
                chunks.push(StoredChunkInfo {
                    chunk_id: chunk_id.to_string(),
                    size_bytes: object.size().unwrap_or(0).max(0) as u64,
                    modified_epoch_ms,
                });
            }

            if response.is_truncated().unwrap_or(false) {
                continuation = response.next_continuation_token().map(ToString::to_string);
                if continuation.is_none() {
                    break;
                }
            } else {
                break;
            }
        }

        chunks.sort_by(|a, b| a.chunk_id.cmp(&b.chunk_id));
        Ok(chunks)
    }

    async fn chunk_exists(&self, chunk_id: &str) -> StoreResult<bool> {
        let key = self.object_key(chunk_id);
        self.client
            .head_object()
            .bucket(&self.bucket)
            .key(&key)
            .send()
            .await
            .map(|_| true)
            .map_err(|e| {
                if e.to_string().contains("NotFound") || e.to_string().contains("NoSuchKey") {
                    RustyStoreError::NotFound(format!("chunk {chunk_id} not found"))
                } else {
                    RustyStoreError::Internal(e.to_string())
                }
            })
            .or_else(|e| match e {
                RustyStoreError::NotFound(_) => Ok(false),
                other => Err(other),
            })
    }

    async fn get_capacity(&self) -> StoreResult<CapacityInfo> {
        // S3 doesn't report per-bucket capacity easily without additional API calls
        Ok(CapacityInfo {
            total_bytes: 0,
            available_bytes: 0,
            used_bytes: 0,
        })
    }

    fn storage_path(&self) -> &Path {
        Path::new("s3://")
    }
}
