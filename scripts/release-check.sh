#!/usr/bin/env bash
# Release validation: formatting, compile, lint, tests, and optional images.
#
# PostgreSQL-backed tests run when TEST_DATABASE_URL or DATABASE_URL is set.
# S3 integration tests run when TEST_S3_ENDPOINT is set.
# Set RELEASE_CHECK_DOCKER=1 to also build the three service images.
set -euo pipefail

cd "$(dirname "$0")/.."

echo "[release] formatting"
cargo fmt --all -- --check

echo "[release] compile"
cargo check --workspace --all-targets

echo "[release] lint"
cargo clippy --workspace --all-targets -- -D warnings

echo "[release] tests"
cargo test --workspace

echo "[release] S3 store unit tests"
cargo test -p storage-engine --features s3 --lib

if [ -n "${TEST_S3_ENDPOINT:-}" ]; then
  echo "[release] S3 integration tests"
  cargo test -p storage-engine --features s3 --test s3_integration
else
  echo "[release] skipping S3 integration tests (TEST_S3_ENDPOINT unset)"
fi

if [ -z "${TEST_DATABASE_URL:-}${DATABASE_URL:-}" ]; then
  echo "[release] note: PostgreSQL/full-stack tests were skipped (TEST_DATABASE_URL unset)"
fi

if [ "${RELEASE_CHECK_DOCKER:-0}" = "1" ]; then
  echo "[release] building container images"
  docker build -f crates/coordinator/Dockerfile -t rustystore-coordinator .
  docker build -f crates/storage-node/Dockerfile -t rustystore-storage-node .
  docker build -f crates/gateway/Dockerfile -t rustystore-gateway .
fi

echo "[release] checks passed"
