//! Metadata queries.
//!
//! All mutating operations run inside a transaction. Incomplete uploads are
//! never visible to readers because `files.status` is only flipped to
//! `committed` inside the finalizing transaction.

use crate::models::*;
use sqlx::postgres::{PgPool, PgPoolOptions};
use uuid::Uuid;

/// Embedded initial schema. Idempotent (`CREATE ... IF NOT EXISTS`) so it can
/// be applied safely on every coordinator startup.
pub const SCHEMA_SQL: &str = include_str!("../../../migrations/init/schema.sql");

/// Metadata repository handle.
pub struct MetadataStore {
    pool: PgPool,
}

impl MetadataStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Connect to PostgreSQL and apply the initial schema (idempotent).
    pub async fn connect_and_migrate(database_url: &str) -> Result<Self, sqlx::Error> {
        let pool = PgPoolOptions::new()
            .max_connections(10)
            .connect(database_url)
            .await?;
        sqlx::raw_sql(SCHEMA_SQL).execute(&pool).await?;
        Ok(Self::new(pool))
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// Cheap connectivity check used by readiness probes.
    pub async fn ping(&self) -> sqlx::Result<()> {
        sqlx::query("SELECT 1")
            .execute(&self.pool)
            .await
            .map(|_| ())
    }

    // ---- nodes -----------------------------------------------------------

    #[allow(clippy::too_many_arguments)]
    pub async fn upsert_node(
        &self,
        node_id: Uuid,
        addr: &str,
        grpc_addr: &str,
        capacity_bytes: i64,
        available_bytes: i64,
        used_bytes: i64,
        lease_expiry: chrono::DateTime<chrono::Utc>,
        version: Option<&str>,
    ) -> sqlx::Result<()> {
        sqlx::query(
            r#"
            INSERT INTO storage_nodes (node_id, addr, grpc_addr, capacity_bytes, available_bytes, used_bytes, lease_expiry, last_seen, healthy, version, updated_at)
            VALUES ($1,$2,$3,$4,$5,$6,$7,now(),true,$8,now())
            ON CONFLICT (node_id) DO UPDATE SET
              addr=EXCLUDED.addr,
              grpc_addr=EXCLUDED.grpc_addr,
              capacity_bytes=EXCLUDED.capacity_bytes,
              available_bytes=EXCLUDED.available_bytes,
              used_bytes=EXCLUDED.used_bytes,
              lease_expiry=EXCLUDED.lease_expiry,
              last_seen=now(),
              healthy=true,
              version=EXCLUDED.version,
              updated_at=now()
            "#,
        )
        .bind(node_id)
        .bind(addr)
        .bind(grpc_addr)
        .bind(capacity_bytes)
        .bind(available_bytes)
        .bind(used_bytes)
        .bind(lease_expiry)
        .bind(version)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn mark_node_unhealthy(&self, node_id: Uuid) -> sqlx::Result<()> {
        sqlx::query("UPDATE storage_nodes SET healthy=false, updated_at=now() WHERE node_id=$1")
            .bind(node_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Refresh a node's lease and capacity counters without clobbering its
    /// address/capacity metadata (used by heartbeats).
    pub async fn touch_node(
        &self,
        node_id: Uuid,
        available_bytes: i64,
        used_bytes: i64,
        lease_expiry: chrono::DateTime<chrono::Utc>,
    ) -> sqlx::Result<()> {
        sqlx::query(
            r#"
            UPDATE storage_nodes
            SET available_bytes=$2, used_bytes=$3, lease_expiry=$4,
                last_seen=now(), healthy=true, updated_at=now()
            WHERE node_id=$1
            "#,
        )
        .bind(node_id)
        .bind(available_bytes)
        .bind(used_bytes)
        .bind(lease_expiry)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Update a node's capacity counters and lease without clobbering its
    /// address metadata (used by explicit capacity reports).
    pub async fn update_node_capacity(
        &self,
        node_id: Uuid,
        capacity_bytes: i64,
        available_bytes: i64,
        used_bytes: i64,
        lease_expiry: chrono::DateTime<chrono::Utc>,
    ) -> sqlx::Result<()> {
        sqlx::query(
            r#"
            UPDATE storage_nodes
            SET capacity_bytes=$2, available_bytes=$3, used_bytes=$4,
                lease_expiry=$5, last_seen=now(), healthy=true, updated_at=now()
            WHERE node_id=$1
            "#,
        )
        .bind(node_id)
        .bind(capacity_bytes)
        .bind(available_bytes)
        .bind(used_bytes)
        .bind(lease_expiry)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn healthy_nodes(&self) -> sqlx::Result<Vec<StorageNodeRow>> {
        let rows =
            sqlx::query_as::<_, StorageNodeRow>("SELECT * FROM storage_nodes WHERE healthy=true")
                .fetch_all(&self.pool)
                .await?;
        Ok(rows)
    }

    pub async fn all_nodes(&self) -> sqlx::Result<Vec<StorageNodeRow>> {
        let rows = sqlx::query_as::<_, StorageNodeRow>("SELECT * FROM storage_nodes")
            .fetch_all(&self.pool)
            .await?;
        Ok(rows)
    }

    /// Expire leases: mark nodes unhealthy whose lease_expiry has passed.
    /// Returns the node IDs that were newly marked unhealthy.
    pub async fn expire_leases(&self) -> sqlx::Result<Vec<Uuid>> {
        let expired: Vec<Uuid> = sqlx::query_as(
            "SELECT node_id FROM storage_nodes WHERE healthy=true AND lease_expiry < now()",
        )
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .map(|(node_id,): (Uuid,)| node_id)
        .collect();

        for node_id in &expired {
            self.mark_node_unhealthy(*node_id).await?;
        }

        Ok(expired)
    }

    /// Find under-replicated chunks: chunks where durable replica count <
    /// the file's replication factor. Returns (chunk_id, file_id, required, actual).
    pub async fn under_replicated_chunks(&self) -> sqlx::Result<Vec<(Uuid, Uuid, i32, i64)>> {
        let rows = sqlx::query_as::<_, (Uuid, Uuid, i32, i64)>(
            r#"
            SELECT c.chunk_id, c.file_id, f.replication_factor,
                   COALESCE(r.durable_count, 0)
            FROM chunks c
            JOIN files f ON c.file_id = f.file_id
            LEFT JOIN (
                SELECT chunk_id, COUNT(*) AS durable_count
                FROM chunk_replicas
                WHERE durable = true
                GROUP BY chunk_id
            ) r ON c.chunk_id = r.chunk_id
            WHERE COALESCE(r.durable_count, 0) < f.replication_factor
            "#,
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Atomically claim a pending replication task: mark it in_progress and
    /// return the task row. Returns None if no pending tasks available.
    pub async fn claim_replication_task(&self) -> sqlx::Result<Option<ReplicationTaskRow>> {
        let mut tx = self.pool.begin().await?;

        let row: Option<ReplicationTaskRow> = sqlx::query_as(
            r#"
            UPDATE replication_tasks
            SET status='in_progress', updated_at=now()
            WHERE task_id = (
                SELECT task_id FROM replication_tasks
                WHERE status='pending'
                ORDER BY created_at ASC
                LIMIT 1
                FOR UPDATE SKIP LOCKED
            )
            RETURNING *
            "#,
        )
        .fetch_optional(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(row)
    }

    pub async fn mark_task_failed(&self, task_id: Uuid, attempts: i32) -> sqlx::Result<()> {
        let status = if attempts >= 5 { "failed" } else { "pending" };
        sqlx::query(
            "UPDATE replication_tasks SET status=$1, attempts=$2, updated_at=now() WHERE task_id=$3",
        )
        .bind(status)
        .bind(attempts)
        .bind(task_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Get the replication factor for a file.
    pub async fn get_file_replication_factor(&self, file_id: Uuid) -> sqlx::Result<Option<i32>> {
        let row: Option<(i32,)> =
            sqlx::query_as("SELECT replication_factor FROM files WHERE file_id=$1")
                .bind(file_id)
                .fetch_optional(&self.pool)
                .await?;
        Ok(row.map(|(rf,)| rf))
    }

    // ---- files -----------------------------------------------------------

    pub async fn create_file(
        &self,
        file_id: Uuid,
        filename: &str,
        total_size: i64,
        chunk_size: i64,
        chunk_count: i32,
        replication_factor: i32,
    ) -> sqlx::Result<()> {
        sqlx::query(
            r#"
            INSERT INTO files (file_id, filename, total_size, chunk_size, chunk_count, replication_factor, status, created_at, updated_at)
            VALUES ($1,$2,$3,$4,$5,$6,'uploading',now(),now())
            "#,
        )
        .bind(file_id)
        .bind(filename)
        .bind(total_size)
        .bind(chunk_size)
        .bind(chunk_count)
        .bind(replication_factor)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Commit a file manifest: flip status to committed inside a transaction.
    pub async fn commit_file(&self, file_id: Uuid) -> sqlx::Result<()> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("UPDATE files SET status='committed', updated_at=now() WHERE file_id=$1 AND status='uploading'")
            .bind(file_id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn get_file(&self, file_id: Uuid) -> sqlx::Result<Option<FileRow>> {
        let row = sqlx::query_as::<_, FileRow>("SELECT * FROM files WHERE file_id=$1")
            .bind(file_id)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row)
    }

    // ---- chunks ----------------------------------------------------------

    pub async fn insert_chunk(
        &self,
        chunk_id: Uuid,
        sha256_hex: &str,
        size_bytes: i64,
        chunk_index: i32,
        file_id: Uuid,
    ) -> sqlx::Result<()> {
        sqlx::query(
            r#"
            INSERT INTO chunks (chunk_id, sha256_hex, size_bytes, chunk_index, file_id)
            VALUES ($1,$2,$3,$4,$5)
            "#,
        )
        .bind(chunk_id)
        .bind(sha256_hex)
        .bind(size_bytes)
        .bind(chunk_index)
        .bind(file_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Insert a chunk and its durable replicas atomically within `tx`.
    ///
    /// Called by `CompleteUpload`: the gateway reports which nodes durably
    /// stored each chunk, so the manifest can later return those replicas.
    pub async fn insert_chunk_with_replicas(
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        chunk_id: Uuid,
        sha256_hex: &str,
        size_bytes: i64,
        chunk_index: i32,
        file_id: Uuid,
        node_ids: &[Uuid],
    ) -> sqlx::Result<()> {
        sqlx::query(
            r#"
            INSERT INTO chunks (chunk_id, sha256_hex, size_bytes, chunk_index, file_id)
            VALUES ($1,$2,$3,$4,$5)
            "#,
        )
        .bind(chunk_id)
        .bind(sha256_hex)
        .bind(size_bytes)
        .bind(chunk_index)
        .bind(file_id)
        .execute(&mut **tx)
        .await?;

        for node_id in node_ids {
            sqlx::query(
                r#"
                INSERT INTO chunk_replicas (chunk_id, node_id, durable)
                VALUES ($1,$2,true)
                ON CONFLICT (chunk_id, node_id) DO NOTHING
                "#,
            )
            .bind(chunk_id)
            .bind(node_id)
            .execute(&mut **tx)
            .await?;
        }

        Ok(())
    }

    pub async fn chunks_for_file(&self, file_id: Uuid) -> sqlx::Result<Vec<ChunkRow>> {
        let rows = sqlx::query_as::<_, ChunkRow>(
            "SELECT * FROM chunks WHERE file_id=$1 ORDER BY chunk_index ASC",
        )
        .bind(file_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Get a single chunk by chunk_id, including its SHA-256 hash.
    pub async fn get_chunk(&self, chunk_id: Uuid) -> sqlx::Result<Option<ChunkRow>> {
        let row = sqlx::query_as::<_, ChunkRow>("SELECT * FROM chunks WHERE chunk_id=$1")
            .bind(chunk_id)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row)
    }

    // ---- replicas --------------------------------------------------------

    pub async fn insert_replica(
        &self,
        chunk_id: Uuid,
        node_id: Uuid,
        durable: bool,
    ) -> sqlx::Result<()> {
        sqlx::query(
            r#"
            INSERT INTO chunk_replicas (chunk_id, node_id, durable)
            VALUES ($1,$2,$3)
            ON CONFLICT (chunk_id, node_id) DO NOTHING
            "#,
        )
        .bind(chunk_id)
        .bind(node_id)
        .bind(durable)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn replicas_for_chunk(&self, chunk_id: Uuid) -> sqlx::Result<Vec<ChunkReplicaRow>> {
        let rows =
            sqlx::query_as::<_, ChunkReplicaRow>("SELECT * FROM chunk_replicas WHERE chunk_id=$1")
                .bind(chunk_id)
                .fetch_all(&self.pool)
                .await?;
        Ok(rows)
    }

    pub async fn durable_replica_count(&self, chunk_id: Uuid) -> sqlx::Result<i64> {
        let row: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM chunk_replicas WHERE chunk_id=$1 AND durable=true",
        )
        .bind(chunk_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(row.0)
    }

    // ---- upload sessions -------------------------------------------------

    pub async fn create_upload_session(
        &self,
        upload_id: Uuid,
        file_id: Option<Uuid>,
    ) -> sqlx::Result<()> {
        sqlx::query(
            "INSERT INTO upload_sessions (upload_id, file_id, status, created_at) VALUES ($1,$2,'pending',now())",
        )
        .bind(upload_id)
        .bind(file_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn abort_upload_session(&self, upload_id: Uuid) -> sqlx::Result<()> {
        sqlx::query(
            "UPDATE upload_sessions SET status='aborted', completed_at=now() WHERE upload_id=$1",
        )
        .bind(upload_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    // ---- replication tasks -----------------------------------------------

    pub async fn create_replication_task(
        &self,
        task_id: Uuid,
        chunk_id: Uuid,
        source_node_id: Option<Uuid>,
        target_node_id: Uuid,
        max_attempts: i32,
    ) -> sqlx::Result<()> {
        sqlx::query(
            r#"
            INSERT INTO replication_tasks (task_id, chunk_id, source_node_id, target_node_id, status, attempts, max_attempts, created_at, updated_at)
            VALUES ($1,$2,$3,$4,'pending',0,$5,now(),now())
            "#,
        )
        .bind(task_id)
        .bind(chunk_id)
        .bind(source_node_id)
        .bind(target_node_id)
        .bind(max_attempts)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn pending_replication_tasks(&self) -> sqlx::Result<Vec<ReplicationTaskRow>> {
        let rows = sqlx::query_as::<_, ReplicationTaskRow>(
            "SELECT * FROM replication_tasks WHERE status='pending' ORDER BY created_at ASC",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn mark_task_in_progress(&self, task_id: Uuid) -> sqlx::Result<()> {
        sqlx::query(
            "UPDATE replication_tasks SET status='in_progress', updated_at=now() WHERE task_id=$1",
        )
        .bind(task_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn mark_task_done(&self, task_id: Uuid) -> sqlx::Result<()> {
        sqlx::query(
            "UPDATE replication_tasks SET status='done', updated_at=now() WHERE task_id=$1",
        )
        .bind(task_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
