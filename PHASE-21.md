# Phase 21: Graceful Shutdown on SIGINT/SIGTERM

## Objective

The gateway claimed graceful shutdown, but it only awaited
`tokio::signal::ctrl_c()` — SIGINT. Docker and Kubernetes stop containers with
**SIGTERM**, so in a real deployment the gateway was killed abruptly. The
coordinator and storage node had no shutdown handling at all: their tonic
servers ran forever and were terminated when the process died.

This phase adds one shared signal future and graceful-stop support to both gRPC
servers.

## What Was Implemented

### `common::shutdown::signal()`
- Resolves on Ctrl+C (SIGINT) on every platform.
- On Unix, also resolves on SIGTERM (`SignalKind::terminate`), selected against
  Ctrl+C.
- Reused by all three services so the behaviour cannot drift.

### Storage node
- `StorageNodeServer::run` now delegates to `run_with_shutdown` with
  `common::shutdown::signal()`.
- `run_with_shutdown(addr, node, shutdown)` binds a `TcpListener` and serves with
  `serve_with_incoming_shutdown`, so tests can pass their own shutdown future.
- `serve_with_listener_shutdown(listener, node, shutdown)` serves on a
  pre-bound listener; the shutdown future stops acceptance and lets in-flight
  RPCs finish. `tokio-stream` (`net`) moved from dev-dependencies to
  dependencies.

### Coordinator
- `CoordinatorServer::run` delegates to `run_with_shutdown`, which uses
  `Server::serve_with_shutdown(addr, signal)`.
- Existing background lease/recovery tasks stop when the runtime is dropped
  after the server future returns.

### Gateway
- Replaced the inline `ctrl_c()` with `common::shutdown::signal()`, so Docker's
  SIGTERM now triggers graceful shutdown.

## Test

`graceful_shutdown_stops_server` (storage node):
1. Serve `NodeService`/`ChunkService`/`ClusterService` on an ephemeral
   `TcpListener` with a `oneshot`-driven shutdown future.
2. Call `Check` over a real client and assert `serving == true`.
3. Drop the client, fire the shutdown, and assert the server task completes
   within 3 seconds.

## Crate Changes

- `crates/common/src/shutdown.rs` — shared signal future
- `crates/common/src/lib.rs`, `Cargo.toml` — export module, add `tokio` (`signal`)
- `crates/storage-node/src/service.rs` — `run_with_shutdown`,
  `serve_with_listener_shutdown`, shutdown test
- `crates/storage-node/src/server.rs` — wrapper delegates shutdown variants
- `crates/storage-node/Cargo.toml` — `tokio-stream` as a runtime dependency
- `crates/coordinator/src/server.rs` — `run_with_shutdown`
- `crates/gateway/src/lib.rs` — shared signal future
- `README.md`

## Verification

- `cargo test --workspace` — 57 pass (was 56)
- `cargo clippy --workspace --all-targets -- -D warnings` — clean
- `cargo fmt --all -- --check` — clean

SIGTERM is Unix-only, so the signal path is exercised on Linux/macOS; the
shutdown *mechanism* is covered by the storage-node test on all platforms.

## What Was Deferred (Future Work)

- Graceful shutdown for the coordinator's auxiliary HTTP server (currently
  aborted with the runtime; it is stateless).
- A configurable drain timeout so a stuck in-flight RPC cannot delay exit
  indefinitely.
- Storage-node heartbeat task signalling shutdown explicitly instead of relying
  on runtime drop.

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
