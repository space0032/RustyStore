# Phase 44: S3 Multipart and Retry Hardening

## Objective

The S3-compatible chunk store used a single `PutObject` call with the AWS
SDK's default retry and timeout behavior. That left two gaps: large chunks had
no multipart path, and transient object-store failures could surface as hard
errors without a consistent retry policy. Phase 44 hardens the store without
changing the `ChunkStore` contract.

## Design

### 1. Retry and timeout policy

`S3ChunkStore::new` configures the SDK with:

- adaptive retry mode and five maximum attempts, covering throttling and
  transient 5xx/connection failures;
- a 60-second per-attempt timeout;
- a 300-second operation timeout.

Every operation (bucket checks, puts, gets, heads, lists, multipart calls)
inherits the policy. Chunk transfers stay bounded, so a stalled object store
fails the affected operation instead of blocking a storage-node request
indefinitely.

### 2. Multipart uploads

Payloads at or below 16 MiB keep the existing single `PutObject` path, which is
the common case for 4–8 MiB chunks. Larger payloads use multipart upload with
8 MiB parts, satisfying S3's 5 MiB minimum for non-final parts:

1. initiate the upload;
2. upload each part sequentially and collect its ETag;
3. complete the upload with the ordered part list;
4. abort the upload if any part or the completion fails.

The part count is validated against S3's 10,000-part limit before initiating.
SHA-256 is still verified before any bytes reach the object store. Multipart
uploads keep the same key and overwrite semantics as single puts.

### 3. Error classification

A shared helper recognizes `NoSuchKey`, `NotFound`, and HTTP 404 responses.
`get_object` still maps missing chunks to `RustyStoreError::NotFound`, while
`head_object` in `chunk_exists` and `delete_chunk` now distinguishes a missing
object from an authentication, connectivity, or service error. Previously any
`head_object` failure was treated as "object missing", which could hide object
store outages from callers.

## Verification

- `cargo fmt --all`
- `cargo check -p storage-engine --features s3 --all-targets`
- `cargo clippy -p storage-engine --features s3 --all-targets -- -D warnings`
- `cargo test -p storage-engine --features s3 --lib`: 11 tests passed
- `cargo test -p storage-engine --features s3 --test s3_integration`: 3 tests
  (skipped locally without `TEST_S3_ENDPOINT`)
- `cargo test --workspace`: all tests passed

Added unit tests for multipart range calculation and an integration test that
round-trips a 17 MiB object through MinIO, exercising three parts. CI provides
MinIO, so the multipart path runs there; local runs without `TEST_S3_ENDPOINT`
skip it.

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
| 32 | Graceful HTTP Shutdown, CI, Container Builds | Complete |
| 33 | Recovery Source Failover | Complete |
| 34 | Recovery Target Failover & Shutdown-Aware Registration | Complete |
| 35 | Full-Stack & S3 Integration Tests | Complete |
| 36 | Recovery Race, Provenance, Priority & Throttle | Complete |
| 37 | Coordinator HA-Lite, Gateway Failover & Image Publishing | Complete |
| 38 | Legacy RPC & Workspace Cleanup | Complete |
| 39 | Dead RPC, Task Recovery & Docs Cleanup | Complete |
| 40 | gRPC TLS/mTLS | Complete |
| 41 | Shared Rate Limiting | Complete |
| 42 | Leader Fencing & Recovery QoS | Complete |
| 43 | Coordinator Registry & Gateway Discovery | Complete |
| 44 | S3 Multipart & Retry Hardening | Complete |
