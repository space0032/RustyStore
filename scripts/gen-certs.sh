#!/usr/bin/env bash
# Generate a local CA, server certificates, and one client identity for gRPC
# mTLS. Output defaults to ./certs (gitignored). Never use these development
# certificates in production.
set -euo pipefail

out="${1:-certs}"
days="${CERT_DAYS:-365}"
mkdir -p "$out"

openssl genrsa -out "$out/ca.key" 4096
openssl req -x509 -new -nodes -key "$out/ca.key" -sha256 -days "$days" \
  -subj "/CN=RustyStore Dev CA" -out "$out/ca.crt"

issue_server() {
  local name="$1" cn="$2" san="$3"
  openssl genrsa -out "$out/$name.key" 2048
  openssl req -new -key "$out/$name.key" -subj "/CN=$cn" -out "$out/$name.csr"
  cat >"$out/$name.ext" <<EOF
subjectAltName=$san
extendedKeyUsage=serverAuth
EOF
  openssl x509 -req -in "$out/$name.csr" -CA "$out/ca.crt" -CAkey "$out/ca.key" \
    -CAcreateserial -out "$out/$name.crt" -days "$days" -sha256 \
    -extfile "$out/$name.ext"
  rm -f "$out/$name.csr" "$out/$name.ext"
}

# Coordinator: storage nodes and gateways connect using this DNS name.
issue_server coordinator coordinator \
  "DNS:coordinator,DNS:localhost,IP:127.0.0.1"

# All storage nodes share a certificate whose SANs cover every Compose node.
issue_server storage-node storage-node-1 \
  "DNS:storage-node-1,DNS:storage-node-2,DNS:storage-node-3,DNS:localhost,IP:127.0.0.1"

# Shared client identity. Use distinct identities per service in production if
# you need authorization/audit by service.
openssl genrsa -out "$out/client.key" 2048
openssl req -new -key "$out/client.key" -subj "/CN=rustystore-client" \
  -out "$out/client.csr"
cat >"$out/client.ext" <<EOF
extendedKeyUsage=clientAuth
EOF
openssl x509 -req -in "$out/client.csr" -CA "$out/ca.crt" -CAkey "$out/ca.key" \
  -CAcreateserial -out "$out/client.crt" -days "$days" -sha256 \
  -extfile "$out/client.ext"
rm -f "$out/client.csr" "$out/client.ext"

rm -f "$out/ca.srl"
echo "certificates written to $out"
