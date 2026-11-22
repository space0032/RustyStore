# Phase 31: Paginated File Listing

## Objective

The gateway could upload, fetch, download, and delete a file by id, but there
was no way to discover which files existed. Clients had to remember ids. Add a
listing endpoint that scales: bounded page sizes, stable ordering, and no
`OFFSET` scans.

## Design

### 1. Keyset pagination in PostgreSQL (`MetadataStore::list_files`)

```sql
SELECT * FROM files
WHERE status = 'committed'
  AND (created_at, file_id) < ($1, $2)   -- omitted on the first page
ORDER BY created_at DESC, file_id DESC
LIMIT $3
```

- Only `committed` files are listed; in-progress and aborted uploads stay
  hidden, matching the `files.status` readability gate used by manifests.
- Ordering is `(created_at DESC, file_id DESC)`. The id tiebreaker makes the
  order total, so rows created in the same instant cannot be skipped or
  repeated.
- A new index `idx_files_status_created` covers `(status, created_at DESC,
  file_id DESC)`.
- `created_at` is bound as a `timestamptz`, not reconstructed from a float, so
  microsecond precision round-trips exactly.

### 2. Opaque cursor (`coordinator::service`)

```
cursor = "<created_at_micros>:<file_id>"
```

- The gateway treats the cursor as opaque and only echoes it back.
- `decode_file_cursor` rejects malformed cursors with `InvalidArgument`; the
  micros value is converted with `DateTime::from_timestamp_micros`, preserving
  the exact instant.
- The service fetches `limit + 1` rows to detect a next page, returns `limit`,
  and emits `next_cursor` only when more rows exist (empty string otherwise).

### 3. gRPC (`ClusterService.ListFiles`)

```proto
rpc ListFiles(ListFilesRequest) returns (ListFilesResponse);

message ListFilesRequest { int32 limit = 1; string cursor = 2; }
message FileSummary {
  string file_id = 1; string filename = 2; int64 total_size = 3;
  int64 chunk_count = 4; int32 replication_factor = 5; string status = 6;
  int64 created_epoch_ms = 7;
}
message ListFilesResponse { repeated FileSummary files = 1; string next_cursor = 2; }
```

`limit <= 0` uses the server default (50); larger values are clamped to 200.
The gateway validates `1..=200` before calling so bad input is a `400`.

### 4. REST (`GET /api/v1/files`)

- `limit` (default 50, max 200) and `cursor` query parameters.
- Response: `{ "files": [...], "next_cursor": "..." }`; `next_cursor` is
  omitted on the last page.
- `POST /api/v1/files` keeps working on the same path (`get(...).post(...)`).
- `docs/api/openapi.yaml` documents the endpoint and its schema.

## Verification

- `cargo fmt --all -- --check` — clean
- `cargo clippy --workspace --all-targets --offline -- -D warnings` — clean
- `cargo test --workspace` — 80 pass (68 unit + 6 integration + 6 PostgreSQL),
  up from 77:
  - `file_cursor_roundtrips`, `file_cursor_rejects_garbage` (coordinator)
  - `list_files_paginates_newest_first_and_hides_uploads` (PostgreSQL)
  - the gateway end-to-end test now lists the uploaded file, asserts the
    `committed` status and absent `next_cursor`, and checks that `limit=0` is a
    `400` before any coordinator call.
- `scripts/test-postgres.ps1` ran the six metadata tests against PostgreSQL 15:
  all passed (~1.3 s).

## What Was Deferred (Future Work)

- Filename/prefix filtering and sorting options.
- Cursor signing or expiry (cursors are read-only and reveal only a timestamp
  and a file id).
- Listing across multiple gateways is already consistent because the cursor is
  derived from the database row, not per-process state.

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
| 31 | Paginated File Listing | Complete |
