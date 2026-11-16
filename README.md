# RustyStore

A fault-tolerant, horizontally scalable Distributed File Storage System written in Rust.

Inspired by Google File System, HDFS, and Amazon S3. Files are divided into fixed-size
chunks, hashed with SHA-256, replicated across storage nodes, and tracked through a
centralized metadata coordinator backed by PostgreSQL.

## Architecture

```text
Client (HTTP) -> Gateway (Axum) <-> Coordinator (Tonic gRPC) <-> Storage Nodes (Tonic gRPC)
                                            |
                                        PostgreSQL (metadata)
```

- **Gateway**: client-facing HTTP REST API, streaming multipart uploads/downloads.
- **Coordinator**: cluster metadata, node health, placement, replication scheduling.
- **Storage Nodes**: persistent chunk storage, atomic writes, inter-node transfers.
- **PostgreSQL**: file/chunk/replica metadata (not file contents).

## Crate Structure

| Crate | Responsibility |
|---|---|
| `proto` | Protocol Buffer definitions + generated tonic code |
| `common` | Shared types, errors, configuration |
| `storage-engine` | Filesystem chunk store, atomic writes, SHA-256 |
| `metadata` | SQLx models + queries against PostgreSQL |
| `coordinator` | Node registration, health, placement, manifest management |
| `storage-node` | Tonic gRPC server for chunk store/retrieve/replicate |
| `gateway` | Axum HTTP gateway, streaming upload/download |
| `replication` | Recovery scheduling, re-replication |
| `observability` | Tracing + Prometheus metrics |

## Prerequisites

- Rust stable toolchain (2024 edition)
- Docker + Docker Compose
- PostgreSQL 15+ (or use docker-compose)

## Quick Start (Docker Compose)

```bash
# 1. Copy environment config
cp .env.example .env

# 2. Build and start the full stack
#    (Postgres, MinIO, Coordinator, 3 Storage Nodes, Gateway)
docker compose up -d
```

The coordinator applies the schema automatically on startup, so no separate
migration step is required.

## Quick Start (local)

```bash
# 1. Start PostgreSQL (and optionally MinIO)
docker compose up -d postgres minio

# 2. Build the service binaries
cargo build --release

# 3. Run each service (separate terminals). Env vars have sane defaults.
DATABASE_URL=postgres://rustystore:rustystore@localhost:5432/rustystore \
  ./target/release/coordinator

COORDINATOR_ADDR=127.0.0.1:50051 \
  ./target/release/storage-node

COORDINATOR_ADDR=127.0.0.1:50051 \
  ./target/release/gateway
```

Service binaries:

| Binary | Default listen | Key environment variables |
|---|---|---|
| `coordinator` | `0.0.0.0:50051` | `DATABASE_URL`, `COORDINATOR_ADDR`, `COORDINATOR_HTTP_ADDR`, `REPLICATION_FACTOR`, `CHUNK_SIZE_BYTES` |
| `storage-node` | `0.0.0.0:50051` | `COORDINATOR_ADDR`, `NODE_ADDR`, `ADVERTISED_ADDR`, `STORAGE_PATH`, `NODE_CAPACITY_BYTES`, `STORAGE_NODE_HTTP_ADDR` |
| `gateway` | `0.0.0.0:8080` | `GATEWAY_ADDR`, `COORDINATOR_ADDR`, `API_KEY`, `REQUIRE_AUTH`, `RATE_LIMIT_RPS`, `RATE_LIMIT_BURST`, `MAX_UPLOAD_BYTES` |

Configuration is validated at startup: an unparsable value or an out-of-range
number (for example `REPLICATION_FACTOR=0` or `MAX_UPLOAD_BYTES=10MB`) exits
non-zero with the offending key instead of silently falling back to a default.

All services handle `SIGINT` (Ctrl+C) and `SIGTERM` (Docker/Kubernetes) and
shut down gracefully, draining in-flight work before exiting.

`DELETE /api/v1/files/{id}` removes the file and chunk metadata first, then
deletes the chunk bytes from the storage nodes that held them. Byte deletion is
best-effort: a failure leaves unreferenced chunks (safe to garbage-collect) but
never metadata pointing at missing data. Deleting an unknown file returns 404.


### Observability

- Gateway: `GET /metrics`, `GET /api/v1/health` (liveness), `GET /api/v1/ready` (coordinator probe) on `GATEWAY_ADDR`
- Coordinator: `GET /metrics`, `GET /health` (liveness), `GET /ready` (DB probe) on `COORDINATOR_HTTP_ADDR` (default `0.0.0.0:8081`)
- Storage node: `GET /metrics`, `GET /health` (liveness), `GET /ready` (storage probe) on `STORAGE_NODE_HTTP_ADDR` (default `0.0.0.0:8082`)

All `/metrics` endpoints expose Prometheus text format. Gateway metrics cover
upload/download bytes and failures; coordinator metrics cover active nodes and
under-replicated chunks; storage nodes report chunks/bytes stored and chunks
deleted.

## Development

```bash
cargo build
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --all -- --check
```

## Testing

```bash
cargo test --workspace
```

## Documentation

- `docs/architecture/` — Mermaid diagrams and data flows
- `docs/adr/` — Architecture Decision Records
- `docs/operations/` — Deployment and troubleshooting

## License

MIT
