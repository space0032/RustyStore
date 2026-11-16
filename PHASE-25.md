# Phase 25: Coordinator-Authoritative Chunk Size and Replication Factor

## Objective

The gateway hardcoded `DEFAULT_CHUNK_SIZE = 4 MB` and `replication_factor() = 3`
and sent them in `CreateUploadSessionRequest`. The coordinator stored the
**request's** values in the `files` row but placed replicas using its **own**
`REPLICATION_FACTOR` and `CHUNK_SIZE_BYTES` configuration. Consequences:

- A coordinator configured with `REPLICATION_FACTOR=2` placed two replicas but
  `GetManifest` reported three (from the gateway's request), so the manifest
  lied about durability.
- The gateway always chunked at 4 MB even if the coordinator was configured for
  a different `CHUNK_SIZE_BYTES`, and the stored `files.chunk_size` disagreed
  with the actual chunking.
- Two places configured the same behavior, with the losing value silently
  recorded.

## The Fix

`CreateUploadSessionResponse` now carries the coordinator's settings:

```proto
message CreateUploadSessionResponse {
  string upload_id = 1;
  string file_id = 2;
  int64 chunk_size = 3;
  int32 replication_factor = 4;
}
```

- The coordinator uses `self.chunk_size_bytes` and `self.replication_factor` for
  the `files` row, so metadata and placement always agree. The request's
  `chunk_size`/`replication_factor` fields are advisory and ignored.
- `CoordinatorClient::create_upload_session` returns
  `(upload_id, file_id, chunk_size, replication_factor)`.
- The gateway removes the hardcoded constants, validates the returned chunk size
  (`> 0`, fits `usize`), and chunks with it. The upload response reports the
  coordinator's values.
- The fake coordinator in the end-to-end test returns **2 MB** chunks and
  `replication_factor = 2` — deliberately different from the old hardcoded
  4 MB/3 — so the test would fail if the gateway ignored the response.

## Tests

`upload_then_download_roundtrip` (extended):

- Uploads 5 MB + 123 bytes.
- Asserts `chunk_count == 3` (2 MB + 2 MB + 1 MB + 123), `chunk_size == 2 MB`,
  and `replication_factor == 2`.
- Asserts three chunk files exist on disk, downloads byte-for-byte, deletes, and
  asserts three replicas were deleted and the directory is empty.

No new test count; the existing end-to-end test now covers the contract.

## Verification

- `cargo test --workspace` — 60 pass
- `cargo clippy --workspace --all-targets -- -D warnings` — clean
- `cargo fmt --all -- --check` — clean

## What Was Deferred (Future Work)

- Rejecting a request whose advisory `chunk_size`/`replication_factor` disagrees
  with the coordinator instead of silently overriding it (useful to catch
  misconfigured clients).
- A `GetConfig` RPC so gateways can learn settings before creating a session.
- Per-upload replication factor overrides with capacity/quorum checks.

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
