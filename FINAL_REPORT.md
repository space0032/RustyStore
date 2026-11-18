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
│   ├── test-postgres.sh    # Provision disposable Postgres + run metadata integration tests
│   ├── test-postgres.ps1   # Same, for Windows
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
├── PHASE-1.md through PHASE-30.md    # Phase-by-phase development log
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
| 17 | Readiness Probes | Complete | `/api/v1/ready`, coordinator `/ready` with DB ping |
| 18 | Storage-Node gRPC Integration Tests | Complete | In-process `ChunkService` tests, `retrieve_chunk` hash fix |
| 19 | Storage-Node HTTP Health/Metrics | Complete | `storage_node::http`, `/health`, `/ready`, `/metrics`, `STORAGE_NODE_HTTP_ADDR` |
| 20 | Validated Startup Configuration | Complete | `common::config` parsers/validators, fail-fast mains |
| 21 | Graceful Shutdown (SIGINT/SIGTERM) | Complete | `common::shutdown`, `serve_with_shutdown` in coordinator + storage node |
| 22 | Storage-Node Transfer Metrics | Complete | `rustystore_storage_{chunks,bytes}_stored_total`, `storage_chunks_deleted_total` |
| 23 | End-to-End Upload/Download Test + ID Fix | Complete | `gateway/tests/e2e_upload_download.rs`, coordinator-issued upload/file IDs |
| 24 | File Deletion End to End | Complete | `DeleteFile` RPC, metadata-first delete, best-effort byte removal, 404 mapping |
| 25 | Coordinator-Authoritative Chunking/Replication | Complete | Chunk size + rf returned by `CreateUploadSession`; gateway honors them |
| 26 | Request-ID Propagation to gRPC | Complete | Task-local id attached to coordinator/storage-node gRPC metadata, incl. spawned tasks |
| 27 | Durable Replica Recording on Commit | Complete | `ChunkManifest.node_ids`, atomic chunk+replica insert, empty replica list rejected |
| 28 | Replication Task Deduplication | Complete | One live task per chunk; stale `in_progress` tasks expire after 5 minutes |
| 29 | Orphan-Chunk GC & Stale Upload Reaper | Complete | `ListChunks` RPC, grace-period collection, TTL reaping of abandoned uploads |
| 30 | PostgreSQL-Backed Integration Tests | Complete | `metadata/tests/postgres_integration.rs`, `scripts/test-postgres.{sh,ps1}` |

Phase documentation: `PHASE-1.md` through `PHASE-30.md`

---

## API Reference

### REST Endpoints

| Method | Endpoint | Description | Auth |
|---|---|---|---|
| `GET` | `/api/v1/health` | Health check | No |
| `POST` | `/api/v1/files` | Upload a file (streaming) | Yes\* |
| `GET` | `/api/v1/files/{file_id}` | Get file metadata | Yes\* |
| `DELETE` | `/api/v1/files/{file_id}` | Delete a file (metadata + best-effort chunk bytes; 404 if absent) | Yes\* |
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

### Tests (66 unit + 6 integration + 5 PostgreSQL-backed = 77 total, all passing)

**common** (11 tests):
1. `parse_value_accepts_valid_integer` — whitespace-trimmed integer parsing
2. `parse_value_rejects_garbage_with_key_context` — typed error names the key/value
3. `parse_addr_value_accepts_valid_address` — address parsing
4. `parse_addr_value_rejects_garbage` — `InvalidAddr` on bad input
5. `require_positive_rejects_zero_and_negatives` — range validation
6. `require_non_empty_rejects_blank` — blank-path validation
7. `current_is_none_outside_scope` — no request id leaks into unrelated tasks
8. `scope_exposes_id` — scoped id is readable
9. `nested_scope_overrides_then_restores` — nested scopes restore the outer id
10. `scope_opt_none_leaves_unset` — `None` does not create a scope
11. `scope_opt_some_sets_id` — `Some` scopes the id

**storage-engine** (9 tests):
1. `test_store_and_retrieve_chunk` — round-trip store/retrieve
2. `test_retrieve_nonexistent_chunk` — NotFound error
3. `test_integrity_mismatch_on_store` — reject mismatched SHA-256 on write
4. `test_integrity_mismatch_on_verify` — detect corruption on read
5. `test_delete_chunk` — delete + idempotent re-delete
6. `test_streaming_reader` — open chunk reader + verify hash
7. `test_get_capacity` — disk capacity query
8. `test_atomic_write_no_partial_visible` — atomic write pattern
9. `test_list_chunks_skips_temp_files` — listing reports stored chunks with size/mtime and ignores in-progress temp files

**coordinator** (15 tests):
1. `test_place_replicas_selects_distinct_nodes` — N distinct nodes
2. `test_place_replicas_insufficient_nodes` — error when not enough nodes
3. `test_place_replicas_excludes_unhealthy_nodes` — skip unhealthy nodes
4. `test_place_replicas_respects_disk_space` — skip nodes with insufficient disk
5. `test_place_replicas_excludes_specified_nodes` — respect exclude list
6. `metrics_render_prometheus_text` — coordinator `/metrics` exposes gauges
7. `health_returns_ok` — coordinator `/health` returns 200 JSON
8. `ready_reports_200_when_dependencies_ok` — readiness 200 when DB reachable
9. `ready_reports_503_when_dependencies_down` — readiness 503 when DB down
10. `server_serves_metrics_over_http` — real `TcpListener` round-trip serves metrics
11. `attaches_current_request_id` — client attaches scoped id as gRPC metadata
12. `omits_header_without_scope` — no metadata when no request id is scoped
13. `deletes_only_old_unreferenced_chunks` — GC keeps referenced and fresh chunks
14. `skips_non_uuid_ids` — GC ignores non-UUID filenames
15. `keeps_chunks_at_exactly_the_cutoff` — grace period is inclusive

**gateway** (20 tests):
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
17. `ready_response_shape` — readiness JSON shape (ready / not_ready)
18. `ready_is_503_when_coordinator_unreachable` — readiness fails closed
19. `coordinator_not_found_maps_to_404` — gRPC NotFound becomes a 404
20. `coordinator_unavailable_maps_to_502` — other gRPC failures become 502

**observability** (1 test):
1. `metrics_render_contains_counters_and_is_idempotent` — exposition contains counters/gauges; registration is idempotent

**storage-node** (10 tests: 5 gRPC + 5 HTTP):
1. `grpc_store_retrieve_delete_roundtrip` — real `ChunkServiceClient` store/retrieve/delete over a `TcpListener`, asserts storage counters advance
2. `grpc_store_rejects_sha_mismatch` — declared SHA-256 that does not match the bytes is rejected
3. `grpc_retrieve_missing_chunk_not_found` — unknown chunk returns gRPC `NotFound`
4. `grpc_list_chunks_reports_stored_chunks` — streaming `ListChunks` returns stored ids, sizes, and mtimes
5. `metrics_render_prometheus_text` — storage node `/metrics` exposes Prometheus text including storage counters
6. `health_returns_ok` — `/health` returns 200 with `service: storage-node`
7. `ready_reports_200_when_storage_ok` — `/ready` 200 when the volume is usable
8. `ready_reports_503_when_storage_down` — `/ready` 503 when the volume probe fails
9. `server_serves_health_over_http` — real `TcpListener` round-trip serves `/health`
10. `graceful_shutdown_stops_server` — server stops accepting after the shutdown future resolves

**metadata (5 tests, require `TEST_DATABASE_URL`)**:
1. `complete_upload_records_chunks_and_durable_replicas` — chunk + durable replica rows committed atomically, manifest readable, no under-replication
2. `deleting_file_cascades_chunks_replicas_sessions_and_tasks` — FK cascades remove chunks, replicas, upload sessions, and replication tasks
3. `replication_task_dedup_and_stale_in_progress` — active-task dedup, abandoned `in_progress` expiry, completed tasks inactive
4. `claim_replication_task_marks_in_progress_once` — `FOR UPDATE SKIP LOCKED` hands a task out exactly once
5. `existing_chunk_ids_and_stale_upload_reaping` — reference lookup + TTL reaping removes abandoned uploads but keeps committed files

These run only when `TEST_DATABASE_URL` (or `DATABASE_URL`) points at a disposable database; otherwise they return early. Provision one with `scripts/test-postgres.sh` / `scripts/test-postgres.ps1`.

**integration** (6 tests):
1. `test_full_chunk_lifecycle` — store, retrieve, verify, delete
2. `test_placement_with_healthy_nodes` — distinct node selection
3. `test_placement_excludes_unhealthy_and_insufficient_disk` — placement constraints
4. `test_duplicate_chunk_dedup_by_hash` — content-addressed dedup
5. `test_corrupted_chunk_detection` — integrity mismatch on verify
6. `upload_then_download_roundtrip` — gateway HTTP upload through a fake coordinator into a real storage node, HTTP download (multi-chunk, byte-for-byte), then delete (metadata + chunk bytes removed, follow-up requests 404); the fake returns a 2 MB chunk size and rf 2 (proving the gateway honors coordinator settings), records replicas from the gateway's `node_ids` (failing if absent), and asserts every coordinator call, including those from the spawned download task, carries the HTTP `x-request-id`

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
- `scripts/test-postgres.sh` / `.ps1` — provision a disposable PostgreSQL, run metadata integration tests, remove the container

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
8. **PostgreSQL integration tests are not part of the default `cargo test` run** — they need `TEST_DATABASE_URL` and a disposable database; `scripts/test-postgres.{sh,ps1}` provisions one with Docker

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
- **Graceful shutdown** — all services handle SIGINT and SIGTERM (Docker/Kubernetes) and drain in-flight work
- **Observability** — tracing (JSON), Prometheus metrics, request tracing
- **Metrics wired end to end** — `/metrics` endpoint, gateway byte/failure counters, coordinator node/under-replication gauges, storage-node chunk/byte store and delete counters
- **Per-IP rate limiting** — token-bucket middleware with `429`/`Retry-After`, health and metrics exempt
- **All services expose Prometheus metrics** — gateway and storage node on `/metrics`, coordinator on a dedicated HTTP port
- **Structured JSON errors & request correlation** — stable error codes plus `x-request-id` on every response, propagated into coordinator and storage-node gRPC metadata
- **Request-ID propagation** — a task-local id is attached to outgoing gRPC calls and re-scoped across `tokio::spawn` boundaries (upload replica fan-out, download streaming)
- **Replica metadata committed with files** — uploads record durable replicas in the same transaction as the chunk and file rows, so manifests are downloadable and deletion can locate chunk bytes
- **Replication task deduplication** — one live task per chunk prevents detector-tick task storms, with stale in-progress tasks expiring so recovery cannot wedge
- **Orphan-chunk GC** — coordinator lists chunks on healthy nodes, subtracts metadata references, and reclaims bytes older than a grace period; abandoned uploads are reaped on a TTL
- **PostgreSQL-backed integration tests** — real SQL paths for replica commit, FK cascades, task dedup/claiming, and GC reaping, provisioned through a disposable Docker database
- **Upload size limits** — `MAX_UPLOAD_BYTES` enforced from content-length and during streaming (`413`)
- **Liveness vs readiness** — `/health` for liveness, `/ready` (DB/coordinator/storage probes) for orchestrators
- **Fail-fast configuration** — invalid or out-of-range env values stop startup instead of silently weakening limits
- **End-to-end upload/download test** — gateway HTTP → fake coordinator → real storage node → HTTP download, no PostgreSQL required; caught a coordinator-issued ID mismatch that would have failed every real upload
- **File deletion** — coordinator removes metadata first (cascade), gateway then deletes chunk bytes from replicas best-effort; missing files return 404
- **Coordinator-authoritative chunking** — the coordinator supplies chunk size and replication factor; the gateway uses them, so manifests match actual placement
- **77 passing tests** across common config, storage engine, coordinator, gateway, storage node (in-process gRPC + HTTP), observability, integration, and PostgreSQL-backed metadata
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

# Run the PostgreSQL-backed metadata tests (Docker required)
scripts/test-postgres.sh        # or scripts/test-postgres.ps1 on Windows

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
