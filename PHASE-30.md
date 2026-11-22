# Phase 30: PostgreSQL-Backed Integration Tests

## Objective

The coordinator's correctness depends on SQL that in-process fakes cannot
exercise: the atomic chunk + replica insert at upload commit, `ON DELETE
CASCADE` behaviour, `FOR UPDATE SKIP LOCKED` task claiming, the abandoned-task
cutoff, and the Phase 29 GC/reaper queries. Until now these paths had only been
run by hand against a real database, and `FINAL_REPORT` listed that gap as a
known limitation.

## Design

### Tests (`crates/metadata/tests/postgres_integration.rs`)

Five tests run against a real PostgreSQL and cover:

1. `complete_upload_records_chunks_and_durable_replicas` — inserts a chunk with
   two replicas in one transaction, marks the file and session `committed`, then
   asserts the manifest, durable replica count, absence of under-replication,
   and the committed status.
2. `deleting_file_cascades_chunks_replicas_sessions_and_tasks` — deletes a file
   and verifies chunks, replicas, upload sessions, and replication tasks all
   disappear through FK cascades.
3. `replication_task_dedup_and_stale_in_progress` — `under_replicated_chunks`
   reports the right required/actual counts, a pending or fresh in-progress task
   blocks duplicates, an `in_progress` task older than five minutes is abandoned,
   and a completed task is inactive.
4. `claim_replication_task_marks_in_progress_once` — the `SKIP LOCKED` claim
   returns the task exactly once and flips it to `in_progress`.
5. `existing_chunk_ids_and_stale_upload_reaping` — reference lookup filters
   unknown ids, `reap_stale_uploads` removes an abandoned `uploading` file and
   its session while keeping a committed file.

### Gating

- Tests read `TEST_DATABASE_URL` (falling back to `DATABASE_URL`). When neither
  is set they return early, so `cargo test --workspace` still passes offline
  and without Docker.
- A `tokio::sync::Mutex` serialises the tests and each starts by truncating the
  metadata tables, so one disposable database is enough and results are
  deterministic despite Rust's parallel runner.
- `crates/metadata` gained a `tokio` dev-dependency (macros + multi-thread).

### Provisioning scripts

- `scripts/test-postgres.sh` and `scripts/test-postgres.ps1` start a
  `postgres:15-alpine` container on port 55432, wait for `pg_isready`, export
  `TEST_DATABASE_URL`, run the test target with `--test-threads=1`, and remove
  the container in a `finally`/`trap`.
- Both accept `PORT`/`IMAGE` overrides.

## Verification

- `cargo fmt --all -- --check` — clean
- `cargo clippy --workspace --all-targets --offline -- -D warnings` — clean
- `cargo test --workspace` — 77 pass (66 unit + 6 integration + 5 PostgreSQL),
  up from 72. Without a database the five new tests return early and still
  report as passing.
- With Docker running, `scripts/test-postgres.ps1` provisioned PostgreSQL 15 and
  ran the five tests against it: all passed in ~2.7 s.
- The container was removed afterwards.

## What Was Deferred (Future Work)

- Running the PostgreSQL tests in CI with a service container (Phase 32).
- End-to-end coordinator tests that drive upload → commit → recovery against a
  real database and storage nodes (the unit tests plus these SQL tests cover the
  pieces individually).
- S3/MinIO integration coverage remains absent.

## Completed Phases

| Phase | Description | Status |
|---|---|---|
| 1 | Architecture & Foundation | Complete |
| 2 | Storage Engine | Complete |
| 3 | Gateway (HTTP API) | Complete |
| 4 | Coordinator (metadata, health, leases) | Complete |
| 5 | Storage Node gRPC | Complete |
| 6 | Replication (durable acks, recovery) | Complete |
| 7 | Fault Tolerance (concurrent recovery) | Complete |
| 8 | Hybrid Storage (S3/MinIO abstraction) | Complete |
| 9 | Testing & Benchmarking | Complete |
| 10 | Production Hardening | Complete |
| 11 | Runnable Services & Deployment | Complete |
| 12 | Metrics Endpoint & Counter Wiring | Complete |
| 13 | Per-IP Rate Limiting | Complete |
| 14 | Coordinator HTTP Metrics/Health | Complete |
| 15 | Structured Errors & Request IDs | Complete |
| 16 | Upload Size Limits | Complete |
| 17 | Readiness Probes | Complete |
| 18 | Storage-Node gRPC Integration Tests | Complete |
| 19 | Storage-Node HTTP Health & Metrics | Complete |
| 20 | Validated Startup Configuration | Complete |
| 21 | Graceful Shutdown (SIGINT/SIGTERM) | Complete |
| 22 | Storage-Node Transfer Metrics | Complete |
| 23 | End-to-End Upload/Download Test + ID Fix | Complete |
| 24 | File Deletion End to End | Complete |
| 25 | Coordinator-Authoritative Chunking/Replication | Complete |
| 26 | Request-ID Propagation to gRPC | Complete |
| 27 | Durable Replica Recording on Commit | Complete |
| 28 | Replication Task Deduplication | Complete |
| 29 | Orphan-Chunk GC & Stale Upload Reaper | Complete |
| 30 | PostgreSQL-Backed Integration Tests | Complete |
