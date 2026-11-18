# Phase 45: Fault Injection, Soak, and Release Validation

## Objective

Phases 1–44 added recovery, HA-lite, TLS, shared limiting, fencing, discovery,
and S3 hardening. Phase 45 turns those capabilities into repeatable release
validation: automated soak coverage, operator-facing load scripts, and a single
local gate that matches the CI checks.

## Design

### 1. Automated soak coverage

`soak_repeated_upload_download_delete_cycles` runs in the full-stack integration
suite against real PostgreSQL, a real coordinator, three storage nodes, and the
gateway router. Each iteration:

- uploads a uniquely sized multi-megabyte payload;
- verifies the reported chunk count;
- downloads the file and compares every byte;
- deletes it and checks the chunk and replica counts;
- confirms the deleted file returns 404.

The default is six iterations, configurable with
`RUSTYSTORE_SOAK_ITERATIONS` (clamped to 1–50). After the loop the test asserts
that no chunk files leaked on any storage node. CI provides PostgreSQL, so this
runs there; local runs skip it without `TEST_DATABASE_URL`.

### 2. Operator soak script

`scripts/soak.sh` exercises a running deployment over HTTP without needing Rust
test tooling. It generates random payloads, uploads them, downloads and verifies
SHA-256, deletes the file, and fails on any non-200 response or checksum
mismatch. Configuration:

- `GATEWAY_URL` (default `http://localhost:8080`);
- `SOAK_ITERATIONS` (default 20);
- `SOAK_SIZE_KB` (default 256).

### 3. Release gate

`scripts/release-check.sh` and `scripts/release-check.ps1` run the same local
gate on Unix-like systems and Windows:

1. `cargo fmt --all -- --check`;
2. `cargo check --workspace --all-targets`;
3. `cargo clippy --workspace --all-targets -- -D warnings`;
4. `cargo test --workspace`;
5. `cargo test -p storage-engine --features s3 --lib`;
6. S3 integration tests when `TEST_S3_ENDPOINT` is set;
7. all three Docker image builds when `RELEASE_CHECK_DOCKER=1`.

The script reports when PostgreSQL/full-stack or S3 integration tests were
skipped so a green run is not mistaken for full external-service coverage.

### 4. CI alignment

The CI S3 step now runs `cargo test -p storage-engine --features s3` instead of
only the integration target, so the Phase 44 multipart range unit tests run in
CI alongside the MinIO round-trip and multipart tests.

Existing fault-injection helpers remain available for container deployments:

- `scripts/crash-node.sh` stops a storage node and restarts it after lease
  expiry;
- `scripts/partition-node.sh` disconnects a node from the Compose network and
  reconnects it.

## Verification

- `cargo fmt --all`
- `cargo check --workspace --all-targets`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`: 113 tests passed
- `cargo test -p storage-engine --features s3 --lib`: 11 tests passed
- Shell scripts syntax-checked with `bash -n`

PostgreSQL/full-stack, MinIO, and Docker validation remain CI/deployment tasks
in this environment because no local PostgreSQL, MinIO, or Docker daemon is
available.

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
| 45 | Fault Injection, Soak & Release Validation | Complete |
