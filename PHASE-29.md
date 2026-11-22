# Phase 29: Orphan-Chunk GC & Stale Upload Reaper

## Objective

Close the only known data-leak class. Chunk bytes could outlive their metadata:

- an upload rejected by `MAX_UPLOAD_BYTES` has already stored earlier chunks;
- a client disconnect leaves chunks the gateway never reports;
- `DELETE /files/{id}` removes metadata first and deletes bytes best-effort;
- a crash between `CreateUploadSession` and `CompleteUpload` leaves a file stuck
  in `uploading` and an upload session pending forever.

None of these bytes are reachable through the API, and nothing reclaimed them.
The `FINAL_REPORT` listed this as a known limitation since Phase 16.

## Design

### 1. Directory listing (`ChunkService.ListChunks`)

```proto
message ListChunksRequest {}
message ChunkSummary {
  string chunk_id = 1;
  int64 size_bytes = 2;
  int64 modified_epoch_ms = 3;
}
rpc ListChunks(ListChunksRequest) returns (stream ChunkSummary);
```

- `storage-engine` gained `ChunkStore::list_chunks() -> Vec<StoredChunkInfo>`,
  implemented for the filesystem store (skips directories and
  `<chunk_id>.tmp.<pid>` in-progress files) and for the S3 store
  (`ListObjectsV2`, paginated, prefix-aware).
- `storage-node` streams the listing; mtime is exposed so the coordinator can
  apply a grace period.

### 2. Coordinator collector (`coordinator::gc`)

Every `ORPHAN_GC_INTERVAL_SECS` (default 300, `0` disables):

1. Reap stale uploads: delete `files` still in `uploading` older than
   `UPLOAD_SESSION_TTL_SECS` (default 24 h), plus file-less pending sessions.
   The FK cascade removes the session; chunk metadata from abandoned uploads
   never existed.
2. For each healthy node: stream `ListChunks`, keep entries older than
   `ORPHAN_GRACE_SECS` (default 1 h), ask PostgreSQL which ids still exist
   (`SELECT ... WHERE chunk_id = ANY($1)`, batched by 1000), and delete the
   remainder on that node.

The pure decision is factored into `orphan_candidates(listed, referenced,
cutoff)` so it is unit-testable without a database.

### Safety

- **Grace period** — a chunk is only collectable if its mtime is at least
  `ORPHAN_GRACE_SECS` old, so chunks of an upload that is still in flight (not
  yet in `chunks`) are never touched.
- **Referenced anywhere = keep** — the check is global (`chunks`), not per node.
  An extra replica of a live chunk is retained; only truly unreferenced bytes
  are removed.
- **Metadata is authoritative** — bytes are deleted only after confirming no
  row references them. The API never points at missing data.
- **UUID filter** — non-UUID filenames (operator debris, temp files that slip
  through) are ignored.
- **Collection is off the request path** and runs sequentially per node; a
  failed node is logged and skipped.

### 3. Metrics

New coordinator counters:

- `rustystore_orphan_chunks_deleted_total`
- `rustystore_orphan_bytes_deleted_total`
- `rustystore_stale_uploads_reaped_total`

## Verification

- `cargo fmt --all -- --check` — clean
- `cargo clippy --workspace --all-targets -- -D warnings` — clean
- `cargo test --workspace` — 72 pass (66 unit + 6 integration), up from 67:
  - `test_list_chunks_skips_temp_files` (storage-engine)
  - `grpc_list_chunks_reports_stored_chunks` (storage-node)
  - `deletes_only_old_unreferenced_chunks`, `skips_non_uuid_ids`,
    `keeps_chunks_at_exactly_the_cutoff` (coordinator GC filter)
- The S3 `list_chunks` implementation is feature-gated; `cargo check` and
  `cargo clippy -- -D warnings` with `--features storage-engine/s3` now pass
  (three pre-existing compile errors in the never-built S3 backend were fixed:
  a wrong `crate::common` path, a `'static` region lifetime, and a
  `Bytes`/`ByteStream` mismatch). Runtime behaviour against MinIO is still
  untested.

## What Was Deferred (Future Work)

- PostgreSQL-backed GC tests (needs a database; see Phase 30).
- Over-replication pruning: extras of a still-referenced chunk are kept.
- `ListChunks` batching/streaming limits for very large nodes.
- End-to-end S3/MinIO tests (the feature now compiles but has no integration
  coverage).

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
