# Phase 11: Runnable Services & Deployment

## Objective

Turn the library-only workspace into deployable artifacts. Until this phase every
crate was a library with no binary entry point, so the system could not actually
be launched and the Docker images referenced a `<service_binary>` placeholder.

This phase adds real service binaries, automatic schema application, a working
node registration + heartbeat client, and fixes the container builds.

## What Was Implemented

### Service binaries
- `crates/coordinator/src/main.rs` — connects to PostgreSQL, applies the schema,
  serves the gRPC Coordinator.
- `crates/storage-node/src/main.rs` — creates the local chunk store, registers
  with the coordinator, streams periodic heartbeats to renew its lease, and
  serves the Node/Chunk/Cluster gRPC services.
- `crates/gateway/src/main.rs` — serves the client-facing HTTP API.

Each binary reads configuration from environment variables with sensible
defaults and initializes JSON tracing via the `observability` crate.

### Automatic schema application
- `migrations/init/schema.sql` is now idempotent (`CREATE TABLE IF NOT EXISTS`,
  `CREATE INDEX IF NOT EXISTS`).
- `MetadataStore::connect_and_migrate(database_url)` embeds the schema with
  `include_str!` and applies it on coordinator startup, so containers no longer
  need a separate `sqlx migrate` step (and the file need not be present at
  runtime).

### Storage-node registration & heartbeat
- `StorageNode::set_node_id()` lets the node adopt the coordinator-assigned UUID.
- `StorageNode::capacity()` exposes live disk capacity for heartbeats.
- The node registers via the client-streaming `NodeService.Register` RPC
  (retrying until the coordinator is reachable) and then runs a bidi
  `NodeService.Heartbeat` stream, reconnecting automatically on failure.

### Correct lease/capacity updates
- Added `MetadataStore::touch_node()` and `update_node_capacity()`.
- The coordinator heartbeat and `ReportCapacity` handlers no longer overwrite a
  node's `addr`/`grpc_addr`/`capacity_bytes` with empty/zero values.

### Container builds
- Rewrote `crates/{coordinator,storage-node,gateway}/Dockerfile` with the real
  binary names, `protobuf-compiler` in the builder stage (required by
  `prost-build`), and a cache-safe build that copies the binary out of the
  BuildKit cache mount before the final stage.

## Environment Variables

| Variable | Service | Default |
|---|---|---|
| `DATABASE_URL` | coordinator | `postgres://rustystore:rustystore@localhost:5432/rustystore` |
| `COORDINATOR_ADDR` | coordinator / node / gateway | `0.0.0.0:50051` / `127.0.0.1:50051` |
| `REPLICATION_FACTOR` | coordinator | `3` |
| `CHUNK_SIZE_BYTES` | coordinator | `4194304` |
| `NODE_ADDR` | storage-node | `0.0.0.0:50051` |
| `ADVERTISED_ADDR` | storage-node | value of `NODE_ADDR` |
| `STORAGE_PATH` | storage-node | `./data/chunks` |
| `NODE_CAPACITY_BYTES` | storage-node | `107374182400` |
| `GATEWAY_ADDR` | gateway | `0.0.0.0:8080` |

## Crate Changes

- `metadata` — `SCHEMA_SQL`, `connect_and_migrate`, `touch_node`, `update_node_capacity`
- `coordinator` — `src/main.rs`; heartbeat/report handlers use the new queries; `observability` dep
- `storage-node` — `src/main.rs`; `set_node_id`, `capacity`; `observability` dep
- `gateway` — `src/main.rs`; `observability` dep
- `migrations/init/schema.sql` — idempotent DDL
- Dockerfiles — real binaries + protoc

## Verification

- `cargo build --workspace --bins` — clean (coordinator, storage-node, gateway binaries produced)
- `cargo clippy --workspace --all-targets -- -D warnings` — clean
- `cargo fmt --all -- --check` — clean
- `cargo test --workspace` — 21/21 tests pass
- Gateway smoke test: launched `gateway.exe`, `GET /api/v1/health` returned
  `{"status":"ok","service":"gateway"}`

## What Was Deferred (Future Work)

- **Metrics endpoint** — Prometheus registry exists; `/metrics` HTTP endpoint and counter wiring still pending
- **TLS/mTLS** — terminate at a reverse proxy or add `tokio-rustls`
- **Rate limiting**, **RBAC**, **JWT** — see Phase 10
- **Automated Docker e2e** — compose stack is runnable; CI-style automated e2e still pending

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

## Build & Run Commands

```bash
# Build all service binaries
cargo build --release

# Run the full stack
docker compose up -d

# Run tests / lints
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```
