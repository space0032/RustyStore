#!/usr/bin/env bash
# Simulate network partition between a storage node and the coordinator.
set -euo pipefail

NODE_NAME="${1:-storage-node-2}"

echo "[partition] isolating $NODE_NAME from network ..."
docker network disconnect rustystore_default "$NODE_NAME" 2>/dev/null || true

echo "[partition] waiting 30s for lease expiry ..."
sleep 30

echo "[partition] reconnecting $NODE_NAME ..."
docker network connect rustystore_default "$NODE_NAME" 2>/dev/null || true

echo "[partition] done."
