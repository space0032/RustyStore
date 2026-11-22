# Phase 32: Graceful HTTP Shutdown, CI, and Container Builds

## Objective

Phase 21 made the gRPC servers drain on `SIGINT`/`SIGTERM`, but the coordinator
and storage-node probe servers (`/metrics`, `/health`, `/ready`) were spawned as
detached tasks with `axum::serve(...)`. A container stop therefore drained gRPC
and then killed the HTTP listeners abruptly. This phase ties both servers to the
same shutdown signal, and adds the missing automation: a GitHub Actions pipeline
and verified container builds.

## Design

### 1. Shared shutdown signal

Both binaries now create one `tokio::sync::watch` channel and a single task that
awaits `common::shutdown::signal()`:

```rust
let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
let signal_tx = shutdown_tx.clone();
tokio::spawn(async move {
    common::shutdown::signal().await;
    let _ = shutdown_tx.send(true);
});

let http_task = tokio::spawn(http::serve_with_shutdown(
    http_addr, ready, wait_for_shutdown(shutdown_rx.clone()),
));

let result = Server::run_with_shutdown(addr, ..., wait_for_shutdown(shutdown_rx)).await;

let _ = signal_tx.send(true);   // also stop HTTP if gRPC failed to start
let _ = http_task.await;        // let axum drain
```

- The same `watch` receiver is cloned for the gRPC shutdown future, so one
  signal stops both listeners.
- If gRPC returns for any other reason (for example a bind failure), the main
  task flips the flag again so the HTTP task cannot outlive the process.
- `wait_for_shutdown` checks the current value first, so a signal that arrives
  before a server starts is not missed.

### 2. `with_graceful_shutdown` on the HTTP surface

`coordinator::http` and `storage_node::http` gained:

```rust
pub async fn serve_with_shutdown(addr, ready, shutdown) -> io::Result<()>
pub async fn serve_listener_with_shutdown(listener, ready, shutdown) -> io::Result<()>
```

The existing `serve` / `serve_listener` helpers remain and pass
`std::future::pending()`, so tests and callers that want process-lifetime
servers are unchanged. The new functions forward to
`axum::serve(listener, router).with_graceful_shutdown(shutdown)`.

Each crate has a `graceful_shutdown_stops_http_server` test: bind an ephemeral
port, serve with a `oneshot` shutdown future, confirm `/health` answers, fire
the signal, await the server, and assert the port no longer accepts connections.

### 3. CI pipeline (`.github/workflows/ci.yml`)

Two jobs:

- **Format, lint, test** — `cargo fmt --all -- --check`,
  `cargo clippy --workspace --all-targets -- -D warnings`, then
  `cargo test --workspace`. A `postgres:15-alpine` service container is
  published on `localhost:5432` and `TEST_DATABASE_URL` points at it, so the six
  PostgreSQL-backed metadata tests run in CI instead of returning early.
  `protobuf-compiler`, `libssl-dev`, and `pkg-config` are installed first, and
  `Swatinem/rust-cache` caches the Cargo build.
- **Build image** — a matrix builds the coordinator, storage-node, and gateway
  Dockerfiles with `docker/build-push-action` (no push), using GitHub Actions
  cache scopes per service.

Concurrency is keyed on the ref with `cancel-in-progress`, so superseded runs
stop early.

### 4. Reproducible container builds

The Dockerfiles previously copied only `Cargo.toml`, `Cargo.lock`, `crates/`,
`proto/`, and `migrations/`, but the workspace also has `benchmarks/` and
`tests/` members. Cargo refuses to load such a workspace, so the images could
not build. Fixes:

- `COPY . .` now brings the complete workspace into the builder.
- A new `.dockerignore` excludes `target`, `.git`, `.github`, `data`, `.env`, and
  `rust-toolchain.toml`. Excluding the toolchain file lets the builder image's
  own stable toolchain run (no second toolchain download); the file still pins
  the host toolchain for developers.
- Base image is `rust:1-slim-bookworm` (latest stable on bookworm, matching the
  `stable` channel and the `debian:bookworm-slim` runtime) instead of
  `rust:1.82-slim`, which predates edition 2024.
- `EXPOSE` now lists both ports: coordinator `50051 8081`, storage node
  `50051 8082`, gateway `8080`.

## Verification

- `cargo fmt --all` — clean
- `cargo clippy --workspace --all-targets -- -D warnings` — clean
- `cargo test --workspace` — 82 pass (70 unit + 6 integration + 6 PostgreSQL),
  up from 80:
  - `coordinator::http::tests::graceful_shutdown_stops_http_server`
  - `storage_node::http::tests::graceful_shutdown_stops_http_server`
- Docker builds succeeded for all three images:
  - `rustystore-coordinator:phase32`
  - `rustystore-gateway:phase32`
  - `rustystore-storage-node:phase32`

  The first build compiled the full dependency graph (~4m40s); the others
  reused the BuildKit registry/target cache (~1m40s each). The build also
  confirmed that the workspace resolves with `benchmarks/` and `tests/`
  present.

## What Was Deferred (Future Work)

- The storage node's registration retry loop does not observe the shutdown
  signal while waiting for the coordinator; a container that never registers
  still relies on the orchestrator's kill timeout. Registration normally
  succeeds in seconds, so this was left out of scope.
- CI builds each image independently, so shared dependencies are compiled once
  per image. A single multi-service builder stage or a prebuilt base image would
  cut CI minutes.
- No image publishing/signing; the workflow builds only.

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
