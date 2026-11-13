# RustyStore — Final Project Report

A fault-tolerant, horizontally scalable Distributed File Storage System written in Rust.
Inspired by Google File System, HDFS, and Amazon S3.

---

## Table of Contents

1. [Overview](#overview)
2. [Architecture](#architecture)
3. [Cargo Workspace Structure](#cargo-workspace-structure)
4. [Phase-by-Phase Implementation](#phase-by-phase-implementation)
5. [API Reference](#api-reference)
6. [Consistency, Durability & Availability Guarantees](#consistency-durability--availability)
7. [Testing & Benchmarks](#testing--benchmarks)
8. [Docker Deployment](#docker-deployment)
9. [Known Limitations](#known-limitations)
10. [Engineering Achievements](#engineering-achievements)

---

## Overview

RustyStore allows clients to upload, download, retrieve, and manage large files while
distributing file data across multiple storage nodes. Files are divided into fixed-size
chunks (default 4 MB), hashed with SHA-256 for integrity, replicated across multiple
storage nodes (default replication factor N=3), and tracked through a centralized
metadata coordinator backed by PostgreSQL.

The system detects storage node failures and automatically restores the required
replication factor using healthy nodes.

---

## Architecture

```text
Client (HTTP) ──┐
                ├─ Gateway (Axum, :8080) ──┐
                                           ├─ gRPC ── Coordinator (:50051) ── PostgreSQL
Client (HTTP) ──┘                            │                                      │
                                             ├─ gRPC streaming ── Storage Node 1 ── local disk
                                             ├─ gRPC streaming ── Storage Node 2 ── local disk
                                             └─ gRPC streaming ── Storage Node 3 ── local disk
```

| Component | Responsibility |
|---|---|
| **Gateway** | Client-facing HTTP REST API, streaming multipart uploads/downloads, auth |
| **Coordinator** | Cluster metadata, node health/leases, placement, replication scheduling |
| **Storage Nodes** | Persistent chunk storage, atomic writes, inter-node data transfers |
| **PostgreSQL** | File/chunk/replica metadata (not file contents) |
| **MinIO** | Optional S3-compatible object storage backend (feature-gated) |

Communication:
- **External (client → gateway)**: HTTP (Axum)
- **Internal (coordinator ↔ storage nodes)**: Tonic gRPC (Protocol Buffers)
- **Metadata**: PostgreSQL via SQLx

---

## Cargo Workspace Structure

```
distributed-storage/
├── Cargo.toml              # Workspace root (12 members)
├── rust-toolchain.toml     # Stable, 2024 edition
├── .env.example            # Environment configuration template
├── docker-compose.yml      # Postgres, MinIO, Coordinator, 3 Storage Nodes, Gateway
├── README.md
├── crates/
│   ├── proto/              # Protocol Buffer definitions + generated tonic code
│   ├── common/             # Shared types, errors, configuration
│   ├── storage-engine/     # Filesystem chunk store (FsChunkStore), S3 store (feature-gated)
│   ├── metadata/           # SQLx models + queries against PostgreSQL
│   ├── coordinator/        # Node registration, health, placement, replication scheduling (+ coordinator binary)
│   ├── storage-node/       # Tonic gRPC server for chunk store/retrieve/replicate (+ storage-node binary)
│   ├── gateway/            # Axum HTTP gateway, streaming upload/download, auth (+ gateway binary)
│   ├── replication/        # Recovery scheduling (placeholder for future expansion)
│   └── observability/      # Tracing + Prometheus metrics setup
├── migrations/
│   └── init/schema.sql     # PostgreSQL initial schema
├── proto/
│   └── storage.proto       # Protocol Buffer service definitions
├── scripts/
│   ├── crash-node.sh       # Fault injection: simulate node crash
│   ├── partition-node.sh   # Fault injection: simulate network partition
│   └── e2e-smoke.sh        # End-to-end smoke test script
├── tests/
│   └── src/lib.rs          # Integration tests (5 tests)
├── benchmarks/
│   └── benches/            # Criterion benchmarks (SHA-256, storage throughput)
├── docs/
│   ├── architecture/overview.md      # 6 Mermaid diagrams
│   ├── adr/ADR-001-centralized-coordinator.md
│   ├── operations/README.md
│   └── api/openapi.yaml              # OpenAPI 3.0 REST API spec
├── PHASE-1.md through PHASE-16.md    # Phase-by-phase development log
└── scripts/                    # Fault injection and smoke test scripts
```

### Crate Dependency Graph (Acyclic)

```
proto  (leaf)
  ↑
common  (proto)
  ↑
storage-engine  (common, sha2)
metadata        (common, sqlx)
coordinator     (proto, metadata, common, tonic, tokio, tracing)
storage-node    (proto, storage-engine, metadata, common, tonic, tokio, tracing)
gateway         (common, proto, coordinator, storage-engine, axum, tower-http, tracing)
replication     (common, metadata, coordinator)
observability   (tracing, prometheus)
```

Rules: no cycles; `proto` is leaf; `common` has no app crate deps; gateway talks to coordinator via a client module inside `coordinator`.

---

## Phase-by-Phase Implementation

| Phase | Description | Status | Key Files |
|---|---|---|---|
| 1 | Architecture & Foundation | Complete | `Cargo.toml`, `proto/storage.proto`, `migrations/init/schema.sql`, `ADR-001` |
| 2 | Storage Engine | Complete | `crates/storage-engine/src/{lib,chunk_store,fs_store}.rs` |
| 3 | Gateway | Complete | `crates/gateway/src/{lib,app,routes,handlers/*}.rs` |
| 4 | Coordinator | Complete | `crates/coordinator/src/{lib,service,placement}.rs` |
| 5 | Storage Node gRPC | Complete | `crates/storage-node/src/{lib,node,service,server}.rs` |
| 6 | Replication | Complete | Coordinator `run_recovery_batch()` + `execute_recovery_task()` |
| 7 | Fault Tolerance | Complete | Semaphore-bounded `JoinSet` recovery, retry logic (5 attempts) |
| 8 | Hybrid Storage | Complete | `crates/storage-engine/src/s3_store.rs` (feature-gated) |
| 9 | Testing & Benchmarks | Complete | `tests/src/lib.rs`, `benchmarks/benches/` |
| 10 | Production Hardening | Complete | `crates/gateway/src/auth.rs`, graceful shutdown, trace/timeout |
| 11 | Runnable Services & Deployment | Complete | `crates/{coordinator,storage-node,gateway}/src/main.rs`, Dockerfiles |
| 12 | Metrics Endpoint & Counter Wiring | Complete | `observability`, `/metrics`, gateway counters, coordinator gauges |
| 13 | Per-IP Rate Limiting | Complete | `gateway::rate_limit`, token bucket middleware, `RATE_LIMIT_*` |
| 14 | Coordinator HTTP Metrics/Health | Complete | `coordinator::http`, `COORDINATOR_HTTP_ADDR`, `/metrics`, `/health` |
| 15 | Structured Errors & Request IDs | Complete | `gateway::error` JSON, `gateway::request_id`, `x-request-id` |
| 16 | Upload Size Limits | Complete | `MAX_UPLOAD_BYTES`, early + streaming enforcement, `413` |

Phase documentation: `PHASE-1.md` through `PHASE-16.md`

---

## API Reference

### REST Endpoints

| Method | Endpoint | Description | Auth |
|---|---|---|---|
| `GET` | `/api/v1/health` | Health check | No |
| `POST` | `/api/v1/files` | Upload a file (streaming) | Yes\* |
| `GET` | `/api/v1/files/{file_id}` | Get file metadata | Yes\* |
| `DELETE` | `/api/v1/files/{file_id}` | Delete a file | Yes\* |
| `GET` | `/api/v1/files/{file_id}/download` | Download a file (streaming) | Yes\* |
| `GET` | `/api/v1/cluster/nodes` | List storage nodes | Yes\* |
| `GET` | `/api/v1/cluster/stats` | Get cluster statistics | Yes\* |
| `GET` | `/metrics` | Prometheus metrics | No |

\* Authentication is configurable via `API_KEY` and `REQUIRE_AUTH` environment variables. Disabled by default for development. `/api/v1/health` and `/metrics` are always public.

### gRPC Services (Internal)

| Service | RPC | Direction |
|---|---|---|
| `NodeService` | `Register`, `Heartbeat`, `ReportCapacity`, `Check` | Storage node → Coordinator |
| `ChunkService` | `StoreChunk` (streaming), `RetrieveChunk` (streaming), `DeleteChunk`, `ReplicateChunk` | Gateway ↔ Storage node |
| `ClusterService` | `AssignReplica`, `NodeHeartbeat`, `CreateUploadSession`, `GetChunkPlan`, `CompleteUpload`, `GetManifest`, `ListNodes`, `GetStats` | Coordinator ↔ Storage node / Gateway |

Full OpenAPI spec: `docs/api/openapi.yaml`

---

## Consistency, Durability & Availability Guarantees

### Consistency
- **Read-after-write** for committed files via Coordinator metadata
- `files.status` gate: only `committed` files are readable; incomplete uploads are invisible
- `chunk_replicas.durable` set true only after fsync + atomic rename + node ack

### Durability
- A replica is durable only after: SHA-256 verified + `fsync` + atomic rename + gRPC ack
- Replication complete only when all N replicas acknowledge durable storage
- **Not zero data loss**: loss of all replicas of a chunk = data loss for that chunk

### Availability
- **Single Coordinator** is a SPOF for new writes (designed for HA evolution)
- Storage nodes continue serving existing chunks during Coordinator outage
- Recovery scheduling runs in the Coordinator process with bounded concurrency (max 4 concurrent)

---

## Testing & Benchmarks

### Unit Tests (33 unit + 5 integration = 38 total, all passing)

**storage-engine** (8 tests):
1. `test_store_and_retrieve_chunk` — round-trip store/retrieve
2. `test_retrieve_nonexistent_chunk` — NotFound error
3. `test_integrity_mismatch_on_store` — reject mismatched SHA-256 on write
4. `test_integrity_mismatch_on_verify` — detect corruption on read
5. `test_delete_chunk` — delete + idempotent re-delete
6. `test_streaming_reader` — open chunk reader + verify hash
7. `test_get_capacity` — disk capacity query
8. `test_atomic_write_no_partial_visible` — atomic write pattern

**coordinator** (8 tests):
1. `test_place_replicas_selects_distinct_nodes` — N distinct nodes
2. `test_place_replicas_insufficient_nodes` — error when not enough nodes
3. `test_place_replicas_excludes_unhealthy_nodes` — skip unhealthy nodes
4. `test_place_replicas_respects_disk_space` — skip nodes with insufficient disk
5. `test_place_replicas_excludes_specified_nodes` — respect exclude list
6. `metrics_render_prometheus_text` — coordinator `/metrics` exposes gauges
7. `health_returns_ok` — coordinator `/health` returns 200 JSON
8. `server_serves_metrics_over_http` — real `TcpListener` round-trip serves metrics

**gateway** (16 tests):
1. `test_chunk_generator_splits_correctly` — bounded chunk splitting (10,000 → 4096/4096/1808)
2. `test_chunk_generator_empty_stream` — empty stream yields no chunks
3. `test_chunk_generator_exact_multiple` — exact multiple yields no trailing empty chunk
4. `disabled_limiter_allows_all` — rate limiting off is a pass-through
5. `burst_is_enforced_per_ip` — token bucket exhausts per IP and isolates clients
6. `not_found_is_404_json` — structured 404 error body
7. `invalid_argument_is_400_json` — structured 400 with detail
8. `internal_is_500_and_grpc_is_502` — 500/502 mapping and codes
9. `payload_too_large_is_413_json` — 413 mapping and code
10. `generates_uuid_when_absent` — request id generated
11. `preserves_incoming_id` — client-supplied id reused
12. `ignores_empty_incoming_id` — empty id replaced
13. `echoes_request_id_on_response` — middleware echoes id on all responses
14. `zero_limit_is_unlimited` — `MAX_UPLOAD_BYTES=0` disables enforcement
15. `at_or_below_limit_is_allowed` — uploads within limit accepted
16. `above_limit_is_rejected` — uploads over limit rejected

**observability** (1 test):
1. `metrics_render_contains_counters_and_is_idempotent` — exposition contains counters/gauges; registration is idempotent

**integration** (5 tests):
1. `test_full_chunk_lifecycle` — store, retrieve, verify, delete
2. `test_placement_with_healthy_nodes` — distinct node selection
3. `test_placement_excludes_unhealthy_and_insufficient_disk` — placement constraints
4. `test_duplicate_chunk_dedup_by_hash` — content-addressed dedup
5. `test_corrupted_chunk_detection` — integrity mismatch on verify

### Benchmarks (Criterion)

**chunk_hashing**:
- `sha256_1kb`, `sha256_4mb`, `sha256_8mb`, `sha256_random_1mb`

**storage_throughput**:
- `store_write_4kb`, `store_write_1mb`, `store_write_4mb`
- `store_read_4mb`, `store_delete_4mb`

Run with: `cargo bench`

### Fault Injection Scripts
- `scripts/crash-node.sh` — stop a storage node, wait for lease expiry, restart, verify recovery
- `scripts/partition-node.sh` — isolate a node from network, wait for lease expiry, reconnect
- `scripts/e2e-smoke.sh` — start stack, run migrations, build services

---

## Docker Deployment

```bash
# 1. Copy environment config
cp .env.example .env

# 2. Build and start the full stack
docker compose up -d
```

The coordinator applies the schema automatically on startup (embedded,
idempotent DDL), so no separate migration step is required.

### Local run (service binaries)

```bash
cargo build --release

DATABASE_URL=postgres://rustystore:rustystore@localhost:5432/rustystore \
  ./target/release/coordinator
COORDINATOR_ADDR=127.0.0.1:50051 ./target/release/storage-node
COORDINATOR_ADDR=127.0.0.1:50051 ./target/release/gateway
```

### Docker Compose Services
- `postgres:15-alpine` — metadata database
- `minio/minio:latest` — S3-compatible object storage (optional backend)
- `coordinator` — cluster metadata + recovery scheduling
- `storage-node-1/2/3` — persistent chunk storage
- `gateway` — client-facing HTTP API

---

## Known Limitations

1. **Single Coordinator SPOF** — new writes require the Coordinator; designed for HA evolution
2. **S3 backend feature-gated** — `cargo build --features storage-engine/s3` required
3. **No source node failover** during recovery — if source dies mid-transfer, task fails and retries
4. **No bandwidth throttling** for replication traffic
5. **No priority queue** — all recovery tasks equal priority (FIFO)
6. **Rate limiting is per-process** — the token bucket lives in each gateway instance and is not shared across replicas
7. **TLS/mTLS not implemented** — recommended at reverse proxy layer
8. **Rejected in-flight uploads leave orphaned chunks** — chunks stored before the size limit is hit are not reclaimed by a sweeper

---

## Engineering Achievements

- **Multi-crate Cargo workspace** with acyclic dependency graph (12 crates)
- **Runnable service binaries** — coordinator, storage-node, gateway with env config + JSON tracing
- **Self-migrating coordinator** — embedded idempotent schema, no external migration step
- **Node registration + heartbeat** — client-streaming register, auto-reconnecting bidi heartbeat lease renewal
- **Atomic chunk writes** — temp file → fsync → rename (no partial reads)
- **SHA-256 integrity** verified on write AND read
- **Lease-based health** — heartbeat ≠ durability proof; nodes unhealthy only after lease expiry
- **Streaming gRPC** — 4–8 MB chunks in 64 KB frames, no unbounded messages
- **Bounded concurrency recovery** — semaphore-limited `JoinSet` (max 4 concurrent)
- **`FOR UPDATE SKIP LOCKED`** — atomic replication task claiming
- **Pluggable storage tier** — filesystem (default) + S3/MinIO (feature-gated)
- **Authentication middleware** — Bearer token validation
- **Graceful shutdown** — Ctrl+C handling with in-flight request completion
- **Observability** — tracing (JSON), Prometheus metrics, request tracing
- **Metrics wired end to end** — `/metrics` endpoint, gateway byte/failure counters, coordinator node/under-replication gauges
- **Per-IP rate limiting** — token-bucket middleware with `429`/`Retry-After`, health and metrics exempt
- **Both services expose Prometheus metrics** — gateway on `/metrics`, coordinator on a dedicated HTTP port
- **Structured JSON errors & request correlation** — stable error codes plus `x-request-id` on every response
- **Upload size limits** — `MAX_UPLOAD_BYTES` enforced from content-length and during streaming (`413`)
- **38 passing tests** across storage engine, coordinator, gateway, observability, and integration
- **Criterion benchmarks** for SHA-256 throughput and storage I/O

---

## Build & Test Commands

```bash
# Build all crates
cargo build --workspace

# Build with S3 backend
cargo build --workspace --features storage-engine/s3

# Run all tests
cargo test --workspace

# Run clippy (zero warnings)
cargo clippy --workspace -- -D warnings

# Check formatting
cargo fmt --all -- --check

# Run benchmarks
cargo bench

# Start infrastructure with Docker Compose
docker compose up -d
```

---

## License

MIT

---

*Generated as part of the RustyStore distributed storage system project. All phases (1–10) implemented and committed.*
