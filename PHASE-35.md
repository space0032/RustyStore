# Phase 35: Full-Stack & S3 Integration Tests

## Objective

Earlier integration tests exercised individual components: the gateway against
a fake coordinator, metadata against PostgreSQL, and the storage engine against
a local filesystem. Nothing started the real coordinator, real storage nodes,
and the gateway together, so cross-service wiring, lease heartbeats, replica
placement, and recovery were only covered by manual Compose runs.

This phase adds full-stack tests that drive the HTTP gateway against a real
coordinator and three real storage nodes backed by PostgreSQL, plus
feature-gated S3/MinIO integration tests.

## Design

### 1. Embeddable coordinator listener

`CoordinatorServer::run_with_shutdown` previously called
`serve_with_shutdown(addr, ...)`, which binds its own socket. Tests need an
ephemeral port and the bound address before startup, so the server now exposes:

```rust
pub async fn serve_with_listener(
    listener: tokio::net::TcpListener,
    store: Arc<MetadataStore>,
    replication_factor: i32,
    chunk_size_bytes: i64,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> Result<(), Box<dyn std::error::Error>>
```

`run_with_shutdown` binds the listener and delegates to it. The implementation
uses `TcpListenerStream` from `tokio-stream` (with the `net` feature) and
`Server::serve_with_incoming_shutdown`.

### 2. Endpoint normalization

Recovery built node endpoints as `http://{grpc_addr}`, while the gateway passed
`grpc_addr` straight to `ChunkServiceClient::connect`. Nodes may register an
address with or without a scheme, so both sides now use a small helper that
only prepends `http://` when no `http(s)://` scheme is present. This keeps the
coordinator and gateway consistent and avoids `http://http://...`.

### 3. Full-stack harness (`tests/tests/full_stack.rs`)

Each test:

- is skipped unless `TEST_DATABASE_URL` (or `DATABASE_URL`) is set;
- serialises with a process-wide `tokio::sync::Mutex`;
- migrates the schema and truncates all metadata tables;
- starts the coordinator on an ephemeral `TcpListener` with a 1 MB chunk size;
- starts three storage nodes in temporary directories, registers each with the
  coordinator, and runs a streaming heartbeat task every 3 seconds;
- waits until all nodes report healthy;
- builds the real gateway router with `AppState::new(coordinator_addr)` and
  drives it with `tower::ServiceExt::oneshot` (no `reqwest` dependency).

Scenarios:

1. `full_stack_upload_download_list_delete` — 3 MB + 123 bytes produces four
   chunks; verifies the response metadata, that 8 replica files exist across the
   nodes, listing, byte-for-byte download, deletion (8 replicas removed), and
   that a follow-up download is 404.
2. `download_survives_a_storage_node_failure` — reads the manifest, stops the
   first replica of the first chunk, then downloads successfully from the other
   replica.
3. `concurrent_uploads_are_all_committed` — four parallel uploads through cloned
   routers all commit and appear in one listing page.
4. `recovery_replaces_a_failed_replica` — stops a replica node, expires its
   lease, and marks its replica non-durable. The coordinator detector and
   scheduler must copy the chunk to the spare node; the test polls the database
   until a durable replacement exists, then downloads the file.

### 4. S3/MinIO integration tests

`S3ChunkStore::ensure_bucket` performs an idempotent `HeadBucket`/`CreateBucket`
so tests and local MinIO runs can bootstrap themselves.

`crates/storage-engine/tests/s3_integration.rs` is compiled only with the `s3`
feature and runs only when `TEST_S3_ENDPOINT` is set. It covers:

- store, existence, retrieve, SHA-256 verification, listing, delete, and
  idempotent re-delete;
- rejection of an upload whose declared SHA-256 does not match the bytes.

Optional environment: `TEST_S3_BUCKET` (default `rustystore-test`),
`TEST_S3_ACCESS_KEY` / `TEST_S3_SECRET_KEY` (default `minioadmin`), and
`TEST_S3_REGION`.

### 5. CI

The test job now also starts a `minio/minio` service container and runs:

```bash
cargo test -p storage-engine --features s3 --test s3_integration
```

with `TEST_S3_*` variables pointing at it, so the S3 path is exercised on every
push.

## Verification

- `cargo fmt --all` — clean
- `cargo clippy --workspace --all-targets -- -D warnings` — clean
- `cargo clippy -p storage-engine --features s3 --all-targets -- -D warnings` —
  clean
- `cargo test --workspace` — 96 pass (80 unit + 10 integration + 6 PostgreSQL),
  up from 90:
  - 4 full-stack tests
  - `node_endpoint_adds_scheme_only_when_missing`
  - `adds_scheme_only_when_missing` (gateway)
- The S3 tests compile and pass their environment gate locally; CI runs them
  against MinIO.

The full-stack and PostgreSQL tests return early when no database is configured.
Provision one with `scripts/test-postgres.sh` / `.ps1`; CI always provides both
PostgreSQL and MinIO.

## What Was Deferred (Future Work)

- Multi-gigabyte streaming uploads, disk exhaustion, database outages, and
  interrupted uploads remain manual/fault-injection scenarios.
- The recovery test relies on the production detector/scheduler intervals
  (10 s / 5 s) and therefore takes up to ~20 s.
- Storage nodes still advertise their bind address (`0.0.0.0:50051`) by default;
  Compose deployments should set a reachable advertised address. This is part of
  the production-hardening backlog.
- S3 tests cover a single bucket; multipart uploads are not implemented.

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
