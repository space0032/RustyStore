#!/usr/bin/env bash
# Fault injection: simulate a storage node crash.
# Usage: ./scripts/crash-node.sh <node_name>
#
# Stops a docker-compose service, waits, then restarts it to verify
# automatic re-replication and lease recovery.

set -euo pipefail

NODE_NAME="${1:-storage-node-1}"

echo "[crash-node] stopping $NODE_NAME ..."
docker compose stop "$NODE_NAME"

echo "[crash-node] waiting 20s for lease expiry / failure detection ..."
sleep 20

echo "[crash-node] checking coordinator health ..."
curl -sf http://localhost:8081/health >/dev/null && echo "coordinator ok" || echo "coordinator down"

echo "[crash-node] restarting $NODE_NAME ..."
docker compose start "$NODE_NAME"

echo "[crash-node] done. Verify replica count via GET /api/v1/cluster/stats"
