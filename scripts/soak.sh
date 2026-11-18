#!/usr/bin/env bash
# Upload, download, and delete repeatedly against a running stack, verifying
# that every downloaded payload matches the uploaded bytes.
#
# Usage:
#   GATEWAY_URL=http://localhost:8080 SOAK_ITERATIONS=50 SOAK_SIZE_KB=512 \
#     ./scripts/soak.sh
set -euo pipefail

GATEWAY_URL="${GATEWAY_URL:-http://localhost:8080}"
ITERATIONS="${SOAK_ITERATIONS:-20}"
SIZE_KB="${SOAK_SIZE_KB:-256}"

WORK_DIR="$(mktemp -d)"
trap 'rm -rf "$WORK_DIR"' EXIT

sha256_file() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | awk '{print $1}'
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$1" | awk '{print $1}'
  else
    openssl dgst -sha256 "$1" | awk '{print $NF}'
  fi
}

if ! command -v curl >/dev/null 2>&1; then
  echo "[soak] curl is required" >&2
  exit 1
fi

echo "[soak] gateway=$GATEWAY_URL iterations=$ITERATIONS size=${SIZE_KB}KiB"

for i in $(seq 1 "$ITERATIONS"); do
  source_file="$WORK_DIR/upload-$i.bin"
  download_file="$WORK_DIR/download-$i.bin"
  head -c $((SIZE_KB * 1024)) /dev/urandom > "$source_file"
  expected="$(sha256_file "$source_file")"

  response="$(curl -fsS --max-time 120 \
    -X POST "$GATEWAY_URL/api/v1/files" \
    -H "x-filename: soak-$i.bin" \
    -H "content-type: application/octet-stream" \
    --data-binary @"$source_file")"

  file_id="$(printf '%s' "$response" | sed -n 's/.*"file_id":"\([^"]*\)".*/\1/p')"
  if [ -z "$file_id" ]; then
    echo "[soak] iteration $i: upload response has no file_id: $response" >&2
    exit 1
  fi

  status="$(curl -sS --max-time 120 -o "$download_file" -w '%{http_code}' \
    "$GATEWAY_URL/api/v1/files/$file_id/download")"
  if [ "$status" != "200" ]; then
    echo "[soak] iteration $i: download returned HTTP $status" >&2
    exit 1
  fi

  actual="$(sha256_file "$download_file")"
  if [ "$actual" != "$expected" ]; then
    echo "[soak] iteration $i: checksum mismatch (expected $expected, got $actual)" >&2
    exit 1
  fi

  status="$(curl -sS --max-time 120 -o /dev/null -w '%{http_code}' \
    -X DELETE "$GATEWAY_URL/api/v1/files/$file_id")"
  if [ "$status" != "200" ]; then
    echo "[soak] iteration $i: delete returned HTTP $status" >&2
    exit 1
  fi

  echo "[soak] iteration $i/$ITERATIONS ok (file_id=$file_id)"
done

echo "[soak] complete: $ITERATIONS iterations passed"
