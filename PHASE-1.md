# Phase 1: Architecture & Foundation

## Objective

Establish the project foundation: requirements analysis, architecture, Cargo workspace, crate responsibilities, Protobuf contracts, and database schema.

## Architecture

```
Client (HTTP) -> Gateway (Axum) <-> Coordinator (Tonic gRPC) <-> Storage Nodes (Tonic gRPC)
                                            |
                                        PostgreSQL (metadata)
```

## Cargo Workspace Structure

```
distributed-storage/
├── Cargo.toml              # workspace root (9 crates)
├── rust-toolchain.toml     # stable, 2024 edition
├── .env.example
├── .gitignore
├── README.md
├── docker-compose.yml      # postgres, minio, coordinator, 3 storage nodes, gateway
├── crates/
│   ├── proto/              # Protocol Buffer definitions + generated tonic code
│   ├── common/             # Shared types, errors, configuration
│   ├── storage-engine/     # Filesystem chunk store, atomic writes, SHA-256
│   ├── metadata/           # SQLx models + queries against PostgreSQL
│   ├── coordinator/        # Node registration, health, placement, manifest management
│   ├── storage-node/       # Tonic gRPC server for chunk store/retrieve/replicate
│   ├── gateway/            # Axum HTTP gateway, streaming upload/download
│   ├── replication/        # Recovery scheduling, re-replication
│   └── observability/      # Tracing + Prometheus metrics
├── migrations/             # PostgreSQL migrations
├── proto/                  # .proto source files
├── scripts/                # fault injection / dev
├── tests/                  # integration tests
├── docs/                   # architecture, adr, operations
└── benchmarks/             # criterion
```

## Crate Responsibilities & Dependency Boundaries

| Crate | Responsibility | Depends On |
|---|---|---|
| `proto` | .proto definitions + tonic codegen | (leaf) |
| `common` | Shared types, errors, config | (leaf) |
| `storage-engine` | Chunk persistence, atomic writes, SHA-256 | common |
| `metadata` | SQLx models + queries | common |
| `coordinator` | Cluster metadata, health, placement | common, metadata, proto |
| `storage-node` | Chunk store/retrieve/replicate gRPC server | common, storage-engine, metadata, proto |
| `gateway` | HTTP API, streaming upload/download | common, proto, coordinator, storage-engine |
| `replication` | Recovery scheduling, re-replication | common, metadata, coordinator |
| `observability` | Tracing + Prometheus metrics | (leaf-ish) |

## Protobuf Contracts (`proto/storage.proto`)

- `NodeService`: Register, Heartbeat, ReportCapacity, Check
- `ChunkService`: StoreChunk (streaming), RetrieveChunk (streaming), DeleteChunk, ReplicateChunk (streaming)
- `ClusterService`: AssignReplica, NodeHeartbeat + gateway RPCs (CreateUploadSession, GetChunkPlan, CompleteUpload, GetManifest, ListNodes, GetStats)
- Streaming gRPC used for chunk transfer (no unbounded unary messages)

## Database Schema (`migrations/init/schema.sql`)

- `storage_nodes` — node registration, leases, health, capacity
- `files` — file manifest with `status` gate (uploading/committed/aborted)
- `chunks` — content-addressed chunks with SHA-256
- `chunk_replicas` — replica placement with `durable` flag
- `upload_sessions` — partial upload tracking for abort cleanup
- `replication_tasks` — recovery work queue with bounded concurrency

## Key Design Decisions

- Atomic write: temp file → fsync → rename (no partial reads)
- Lease-based health (heartbeat ≠ durability proof)
- `files.status` gate — incomplete uploads never readable
- Streaming gRPC for chunks (4–8 MB exceeds safe unary limits)
- Replica durable only after node ack (fsync + rename)
- `UNIQUE(chunk_id, node_id)` prevents duplicate replica rows
- Content-addressed chunk IDs (SHA-256) for dedup + integrity

## Consistency / Durability / Availability

- **Consistency**: Read-after-write for committed files via Coordinator metadata
- **Durability**: Replica durable after fsync + atomic rename + ack; not zero data loss
- **Availability**: Single Coordinator is SPOF in Phase 1; designed to evolve to HA

## ADR-001

Centralized Coordinator with planned HA evolution (metadata in Postgres enables future leader election).

## Verification

- `cargo check --workspace` — clean
- `cargo clippy --workspace -D warnings` — clean
- `cargo fmt --all -- --check` — clean

## Next (Phase 2)

Storage Engine: filesystem abstraction, chunk persistence, atomic writes, SHA-256 verification, storage capacity tracking.
