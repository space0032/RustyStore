# Operations

## Running locally

```bash
cp .env.example .env
docker compose up -d postgres minio
sqlx migrate run --source ./migrations --database-url "$DATABASE_URL"
cargo build --release
```

## Health checks

- `GET /api/v1/health` on the gateway
- gRPC `Check` on each storage node and the coordinator

## Simulating failure

```bash
./scripts/crash-node.sh storage-node-2
./scripts/partition-node.sh storage-node-3
```

## Metrics

Prometheus scrape target: `http://<gateway>:9090/metrics` (exposed by observability
crate; wire into main in Phase 10).
