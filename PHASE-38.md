# Phase 38: Legacy RPC & Workspace Cleanup

## Objective

Several interfaces survived earlier refactors as stubs or duplicates: the
storage node still advertised `NodeService` and `ClusterService` methods it
never implemented, `ClusterService` still exposed coordinator-only
`AssignReplica`/`NodeHeartbeat` RPCs, the coordinator kept a dead single-cycle
recovery function, and workspace metadata carried stale or duplicated settings.
This phase removes that surface area and makes the workspace configuration
consistent.

## Design

### 1. Remove unused coordinator-to-node RPCs

`proto/storage.proto` no longer declares `AssignReplica` or `NodeHeartbeat`.
Replication is orchestrated by the coordinator's recovery scheduler through
`ChunkService::RetrieveChunk` and `StoreChunk`, so these RPCs had no callers.

`ClusterService` now contains only the gateway-facing API:

- `CreateUploadSession`
- `GetChunkPlan`
- `CompleteUpload`
- `GetManifest`
- `DeleteFile`
- `ListFiles`
- `ListNodes`
- `GetStats`

The coordinator's `ClusterService` implementation drops the corresponding
associated stream type and stub methods.

### 2. Storage nodes expose only `ChunkService`

`StorageNodeServer` previously registered three services, but the storage node
only implements chunk storage and transfer. `NodeService` and `ClusterService`
were stubs returning `unimplemented`, and the storage-node `register` RPC was
unused because registration is performed by the node binary as a client.

The server now registers only `ChunkServiceServer`. The gRPC shutdown test uses
`ChunkService::ListChunks` to confirm the server is accepting calls before
shutdown, replacing the old `NodeService::Check` probe.

### 3. Remove dead coordinator code

`Coordinator::run_recovery_cycle` was an unused single-cycle predecessor of the
current recovery batch scheduler. It is deleted along with the empty
`CoordinatorServer` placeholder in `service.rs`; the real runner lives in
`server.rs`.

### 4. Consistent workspace metadata

Every member now inherits `version`, `edition`, and `rust-version` from
`[workspace.package]`:

```toml
[workspace.package]
version = "0.1.0"
edition = "2024"
rust-version = "1.85"
```

Edition 2024 requires Rust 1.85, so the previous `1.82` value was incorrect.
The duplicate `[profile.release]` in `benchmarks/Cargo.toml` is removed because
Cargo ignores non-root package profiles and warns about them.

### 5. Compose cleanup

The obsolete Compose `version` field is gone, and storage-node services no
longer receive `DATABASE_URL`; only the coordinator connects to PostgreSQL.

## Verification

- `cargo fmt --all`
- `cargo check --workspace --all-targets`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`: 105 tests passed

No new tests were needed: the existing full-stack, coordinator, gateway, and
storage-node suites cover the remaining RPC surface. PostgreSQL-backed and
full-stack tests still return early locally without a database and run in CI.

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
