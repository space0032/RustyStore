//! PostgreSQL-backed integration tests for `MetadataStore`.
//!
//! These exercise the real SQL paths that in-process fakes cannot cover:
//! replica rows written during upload commit, FK cascades on delete,
//! under-replication detection, replication-task deduplication, and the
//! orphan/upload-reaper helpers.
//!
//! They run only when `TEST_DATABASE_URL` (or `DATABASE_URL`) is set and point
//! at a disposable database. `scripts/test-postgres.ps1` / `.sh` provision one
//! with Docker. Without a URL the tests return early so a plain
//! `cargo test --workspace` still works offline.

use metadata::MetadataStore;
use std::sync::OnceLock;
use tokio::sync::{Mutex, MutexGuard};
use uuid::Uuid;

fn database_url() -> Option<String> {
    std::env::var("TEST_DATABASE_URL")
        .or_else(|_| std::env::var("DATABASE_URL"))
        .ok()
}

fn lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

/// Connect, migrate, and start each test from empty tables. The mutex keeps
/// the shared database deterministic despite Rust's parallel test runner.
async fn setup() -> Option<(MetadataStore, MutexGuard<'static, ()>)> {
    let url = database_url()?;
    let guard = lock().lock().await;
    let store = MetadataStore::connect_and_migrate(&url)
        .await
        .expect("connect to TEST_DATABASE_URL");

    sqlx::raw_sql(
        "TRUNCATE replication_tasks, chunk_replicas, chunks, upload_sessions, files, storage_nodes CASCADE",
    )
    .execute(store.pool())
    .await
    .expect("truncate");

    Some((store, guard))
}

async fn node(store: &MetadataStore) -> Uuid {
    let node_id = Uuid::new_v4();
    store
        .upsert_node(
            node_id,
            "127.0.0.1:9000",
            "127.0.0.1:50051",
            1_000_000,
            1_000_000,
            0,
            chrono::Utc::now() + chrono::Duration::hours(1),
            Some("test"),
        )
        .await
        .expect("insert node");
    node_id
}

async fn file(store: &MetadataStore, replication_factor: i32) -> Uuid {
    let file_id = Uuid::new_v4();
    store
        .create_file(file_id, "test.bin", 12, 4, 3, replication_factor)
        .await
        .expect("insert file");
    file_id
}

fn sha() -> String {
    "a".repeat(64)
}

async fn insert_chunk(store: &MetadataStore, file_id: Uuid, chunk_id: Uuid, node_ids: &[Uuid]) {
    let mut tx = store.pool().begin().await.expect("begin");
    MetadataStore::insert_chunk_with_replicas(&mut tx, chunk_id, &sha(), 4, 0, file_id, node_ids)
        .await
        .expect("insert chunk + replicas");
    tx.commit().await.expect("commit");
}

#[tokio::test]
async fn complete_upload_records_chunks_and_durable_replicas() {
    let Some((store, _guard)) = setup().await else {
        return;
    };
    let n1 = node(&store).await;
    let n2 = node(&store).await;
    let file_id = file(&store, 2).await;
    let upload_id = Uuid::new_v4();
    store
        .create_upload_session(upload_id, Some(file_id))
        .await
        .unwrap();

    let chunk_id = Uuid::new_v4();
    let mut tx = store.pool().begin().await.unwrap();
    MetadataStore::insert_chunk_with_replicas(&mut tx, chunk_id, &sha(), 4, 0, file_id, &[n1, n2])
        .await
        .unwrap();
    sqlx::query("UPDATE files SET status='committed', updated_at=now() WHERE file_id=$1")
        .bind(file_id)
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE upload_sessions SET status='committed', completed_at=now() WHERE upload_id=$1",
    )
    .bind(upload_id)
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();

    let chunks = store.chunks_for_file(file_id).await.unwrap();
    assert_eq!(chunks.len(), 1);
    assert_eq!(chunks[0].chunk_id, chunk_id);
    assert_eq!(chunks[0].sha256_hex.trim(), sha());

    let replicas = store.replicas_for_chunk(chunk_id).await.unwrap();
    assert_eq!(replicas.len(), 2, "both gateway-reported nodes recorded");
    assert!(replicas.iter().all(|r| r.durable));
    assert_eq!(store.durable_replica_count(chunk_id).await.unwrap(), 2);

    assert!(
        store.under_replicated_chunks().await.unwrap().is_empty(),
        "a committed file with rf replicas is not under-replicated"
    );

    let committed = store.get_file(file_id).await.unwrap().unwrap();
    assert_eq!(committed.status, "committed");
}

#[tokio::test]
async fn deleting_file_cascades_chunks_replicas_sessions_and_tasks() {
    let Some((store, _guard)) = setup().await else {
        return;
    };
    let n1 = node(&store).await;
    let n2 = node(&store).await;
    let file_id = file(&store, 2).await;
    let chunk_id = Uuid::new_v4();
    insert_chunk(&store, file_id, chunk_id, &[n1, n2]).await;

    let upload_id = Uuid::new_v4();
    store
        .create_upload_session(upload_id, Some(file_id))
        .await
        .unwrap();
    let task_id = Uuid::new_v4();
    store
        .create_replication_task(task_id, chunk_id, Some(n1), n2, 5, 0)
        .await
        .unwrap();

    sqlx::query("DELETE FROM files WHERE file_id=$1")
        .bind(file_id)
        .execute(store.pool())
        .await
        .unwrap();

    assert!(store.get_file(file_id).await.unwrap().is_none());
    assert!(store.chunks_for_file(file_id).await.unwrap().is_empty());
    assert!(store.replicas_for_chunk(chunk_id).await.unwrap().is_empty());

    let sessions: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM upload_sessions WHERE upload_id=$1")
            .bind(upload_id)
            .fetch_one(store.pool())
            .await
            .unwrap();
    assert_eq!(sessions, 0, "upload session cascades with the file");

    let tasks: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM replication_tasks WHERE chunk_id=$1")
        .bind(chunk_id)
        .fetch_one(store.pool())
        .await
        .unwrap();
    assert_eq!(tasks, 0, "replication tasks cascade with the chunk");
}

#[tokio::test]
async fn replication_task_dedup_and_stale_in_progress() {
    let Some((store, _guard)) = setup().await else {
        return;
    };
    let n1 = node(&store).await;
    let n2 = node(&store).await;
    let _n3 = node(&store).await;
    let file_id = file(&store, 3).await;
    let chunk_id = Uuid::new_v4();
    insert_chunk(&store, file_id, chunk_id, &[n1]).await;

    let under = store.under_replicated_chunks().await.unwrap();
    assert_eq!(under.len(), 1);
    assert_eq!(under[0].0, chunk_id);
    assert_eq!(under[0].2, 3, "required replicas comes from the file");
    assert_eq!(under[0].3, 1, "only the durable replica counts");

    let task_id = Uuid::new_v4();
    store
        .create_replication_task(task_id, chunk_id, Some(n1), n2, 5, 0)
        .await
        .unwrap();
    assert!(store.has_active_replication_task(chunk_id).await.unwrap());

    // A fresh in-progress task blocks duplicates.
    store.mark_task_in_progress(task_id).await.unwrap();
    assert!(store.has_active_replication_task(chunk_id).await.unwrap());

    // An in-progress task older than five minutes is considered abandoned.
    sqlx::query(
        "UPDATE replication_tasks SET updated_at = now() - INTERVAL '10 minutes' WHERE task_id=$1",
    )
    .bind(task_id)
    .execute(store.pool())
    .await
    .unwrap();
    assert!(!store.has_active_replication_task(chunk_id).await.unwrap());

    store.mark_task_done(task_id).await.unwrap();
    assert!(!store.has_active_replication_task(chunk_id).await.unwrap());
}

#[tokio::test]
async fn claim_replication_task_marks_in_progress_once() {
    let Some((store, _guard)) = setup().await else {
        return;
    };
    let n1 = node(&store).await;
    let n2 = node(&store).await;
    let file_id = file(&store, 2).await;
    let chunk_id = Uuid::new_v4();
    insert_chunk(&store, file_id, chunk_id, &[n1]).await;

    let task_id = Uuid::new_v4();
    store
        .create_replication_task(task_id, chunk_id, Some(n1), n2, 5, 0)
        .await
        .unwrap();

    let claimed = store
        .claim_replication_task()
        .await
        .unwrap()
        .expect("pending task is claimed");
    assert_eq!(claimed.task_id, task_id);
    assert_eq!(claimed.status, "in_progress");
    assert!(
        store.claim_replication_task().await.unwrap().is_none(),
        "a claimed task is not handed out twice"
    );
}

#[tokio::test]
async fn list_files_paginates_newest_first_and_hides_uploads() {
    let Some((store, _guard)) = setup().await else {
        return;
    };

    // Five committed files with deterministic, strictly increasing times.
    let mut ids = Vec::new();
    for i in 0..5 {
        let file_id = Uuid::new_v4();
        store
            .create_file(file_id, &format!("f{i}.bin"), 10, 4, 3, 1)
            .await
            .unwrap();
        sqlx::query(
            "UPDATE files SET status='committed', created_at=to_timestamp($2) WHERE file_id=$1",
        )
        .bind(file_id)
        .bind(1_700_000_000.0 + i as f64)
        .execute(store.pool())
        .await
        .unwrap();
        ids.push(file_id);
    }

    // An in-progress upload must not appear.
    let draft = Uuid::new_v4();
    store
        .create_file(draft, "draft.bin", 10, 4, 3, 1)
        .await
        .unwrap();

    let page1 = store.list_files(2, None).await.unwrap();
    assert_eq!(
        page1
            .iter()
            .map(|f| f.filename.as_str())
            .collect::<Vec<_>>(),
        vec!["f4.bin", "f3.bin"]
    );

    let cursor1 = (page1[1].created_at, page1[1].file_id);
    let page2 = store.list_files(2, Some(cursor1)).await.unwrap();
    assert_eq!(
        page2
            .iter()
            .map(|f| f.filename.as_str())
            .collect::<Vec<_>>(),
        vec!["f2.bin", "f1.bin"]
    );

    let cursor2 = (page2[1].created_at, page2[1].file_id);
    let page3 = store.list_files(2, Some(cursor2)).await.unwrap();
    assert_eq!(
        page3
            .iter()
            .map(|f| f.filename.as_str())
            .collect::<Vec<_>>(),
        vec!["f0.bin"]
    );

    // No duplicates or gaps across the three pages.
    let mut seen: Vec<Uuid> = page1
        .iter()
        .chain(&page2)
        .chain(&page3)
        .map(|f| f.file_id)
        .collect();
    let total = seen.len();
    seen.sort();
    seen.dedup();
    assert_eq!(total, 5);
    assert_eq!(seen.len(), 5);
    assert!(seen.iter().all(|id| ids.contains(id)));

    // An empty page past the end is not an error.
    let cursor3 = (page3[0].created_at, page3[0].file_id);
    assert!(store.list_files(2, Some(cursor3)).await.unwrap().is_empty());
}

#[tokio::test]
async fn existing_chunk_ids_and_stale_upload_reaping() {
    let Some((store, _guard)) = setup().await else {
        return;
    };
    let n1 = node(&store).await;
    let file_id = file(&store, 1).await;
    let chunk_id = Uuid::new_v4();
    insert_chunk(&store, file_id, chunk_id, &[n1]).await;

    let unknown = Uuid::new_v4();
    let existing = store
        .existing_chunk_ids(&[chunk_id, unknown])
        .await
        .unwrap();
    assert_eq!(existing, vec![chunk_id]);

    // A committed file survives the reaper.
    sqlx::query("UPDATE files SET status='committed', updated_at=now() WHERE file_id=$1")
        .bind(file_id)
        .execute(store.pool())
        .await
        .unwrap();

    // An abandoned upload (file stuck in `uploading`) is removed with its
    // session via the FK cascade.
    let stale_file = Uuid::new_v4();
    store
        .create_file(stale_file, "stale.bin", 8, 4, 2, 1)
        .await
        .unwrap();
    let stale_session = Uuid::new_v4();
    store
        .create_upload_session(stale_session, Some(stale_file))
        .await
        .unwrap();
    sqlx::query("UPDATE files SET updated_at = now() - INTERVAL '2 hours' WHERE file_id=$1")
        .bind(stale_file)
        .execute(store.pool())
        .await
        .unwrap();

    let reaped = store.reap_stale_uploads(3600).await.unwrap();
    assert_eq!(reaped, 1, "only the stale uploading file is removed");
    assert!(store.get_file(stale_file).await.unwrap().is_none());
    assert!(store.get_file(file_id).await.unwrap().is_some());

    let sessions: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM upload_sessions WHERE upload_id=$1")
            .bind(stale_session)
            .fetch_one(store.pool())
            .await
            .unwrap();
    assert_eq!(sessions, 0, "stale session cascades with its file");
}

#[tokio::test]
async fn replication_task_priority_order_and_actual_nodes() {
    let Some((store, _guard)) = setup().await else {
        return;
    };
    let n1 = node(&store).await;
    let n2 = node(&store).await;
    let n3 = node(&store).await;
    let file_id = file(&store, 2).await;
    let chunk_id = Uuid::new_v4();
    insert_chunk(&store, file_id, chunk_id, &[n1]).await;

    let low = Uuid::new_v4();
    store
        .create_replication_task(low, chunk_id, Some(n1), n2, 5, 1)
        .await
        .unwrap();
    let high = Uuid::new_v4();
    store
        .create_replication_task(high, chunk_id, Some(n1), n3, 5, 5)
        .await
        .unwrap();

    let claimed = store
        .claim_replication_task()
        .await
        .unwrap()
        .expect("pending task is claimed");
    assert_eq!(claimed.task_id, high, "higher priority is claimed first");
    assert_eq!(claimed.priority, 5);

    store
        .mark_task_done_with_nodes(high, Some(n2), n3)
        .await
        .unwrap();
    let (status, source, target): (String, Option<Uuid>, Uuid) = sqlx::query_as(
        "SELECT status, source_node_id, target_node_id FROM replication_tasks WHERE task_id=$1",
    )
    .bind(high)
    .fetch_one(store.pool())
    .await
    .unwrap();
    assert_eq!(status, "done");
    assert_eq!(source, Some(n2), "actual source is persisted");
    assert_eq!(target, n3, "actual target is persisted");
}
