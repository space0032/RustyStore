#!/usr/bin/env bash
# Provisions a disposable PostgreSQL, runs the metadata integration tests, and
# removes the container. Requires Docker.
set -euo pipefail

PORT="${PORT:-55432}"
IMAGE="${IMAGE:-postgres:15-alpine}"
NAME="rustystore-test-pg"

docker rm -f "$NAME" >/dev/null 2>&1 || true
docker run -d --name "$NAME" \
    -e POSTGRES_USER=rustystore \
    -e POSTGRES_PASSWORD=rustystore \
    -e POSTGRES_DB=rustystore_test \
    -p "${PORT}:5432" \
    "$IMAGE" >/dev/null

cleanup() {
    docker rm -f "$NAME" >/dev/null 2>&1 || true
}
trap cleanup EXIT

for _ in $(seq 1 60); do
    if docker exec "$NAME" pg_isready -U rustystore -d rustystore_test >/dev/null 2>&1; then
        break
    fi
    sleep 1
done

export TEST_DATABASE_URL="postgres://rustystore:rustystore@localhost:${PORT}/rustystore_test"
cargo test -p metadata --test postgres_integration -- --test-threads=1
