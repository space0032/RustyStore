# Operations

## Running locally

```bash
cp .env.example .env
docker compose up -d postgres minio
cargo run -p coordinator
cargo run -p storage-node
cargo run -p gateway
```

The coordinator applies the embedded schema on startup; no separate migration
step is required. `docker compose up --build` starts the full stack.

## Health checks

- Gateway: `GET /api/v1/health` (liveness), `GET /api/v1/ready` (coordinator probe)
- Coordinator: `GET /health` (liveness), `GET /ready` (database probe) on port 8081
- Storage node: `GET /health` (liveness), `GET /ready` (storage volume probe) on port 8082

## Simulating failure

```bash
./scripts/crash-node.sh storage-node-2
./scripts/partition-node.sh storage-node-3
```

## Metrics

Prometheus scrapes the HTTP port of each service:

- Gateway: `http://<gateway>:8080/metrics`
- Coordinator: `http://<coordinator>:8081/metrics`
- Storage node: `http://<storage-node>:8082/metrics`

All endpoints expose Prometheus text format. Gateway metrics cover upload and
download bytes/failures; coordinator metrics cover active nodes,
under-replicated chunks, leadership state, reclaimed orphan chunks/bytes, and
reaped stale uploads; storage nodes report chunks/bytes stored and deleted.
