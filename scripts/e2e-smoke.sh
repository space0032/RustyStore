#!/usr/bin/env bash
# End-to-end smoke test script.
set -euo pipefail

echo "[e2e] starting stack ..."
docker compose up -d

echo "[e2e] waiting for postgres ..."
for i in $(seq 1 30); do
  docker compose exec -T postgres pg_isready -U rustystore -d rustystore && break
  sleep 2
done

echo "[e2e] running migrations ..."
sqlx migrate run --source ./migrations --database-url "postgres://rustystore:rustystore@localhost:5432/rustystore"

echo "[e2e] building services ..."
cargo build --release

echo "[e2e] start coordinator, storage nodes, gateway manually or via compose"
echo "[e2e] then run: cargo test --workspace"
