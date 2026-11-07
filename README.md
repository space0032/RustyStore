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

# 2. Start infrastructure
docker compose up -d postgres minio

# 3. Run migrations (sqlx-cli)
# cargo install sqlx-cli
sqlx migrate run --source ./migrations --database-url "$DATABASE_URL"

# 4. Build and run services
cargo build --release
# then run coordinator, storage-node(s), gateway separately
```

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
