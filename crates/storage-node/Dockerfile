# RustyStore storage node image.
# Usage: docker build -f crates/storage-node/Dockerfile -t rustystore-storage-node .

FROM rust:1-slim-bookworm AS builder
WORKDIR /build
RUN apt-get update && apt-get install -y pkg-config libssl-dev protobuf-compiler && rm -rf /var/lib/apt/lists/*
# The workspace has members under crates/, benchmarks/, and tests/, so the
# whole context is needed for Cargo to resolve the workspace.
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/build/target \
    cargo build --release -p storage-node --bin storage-node && \
    cp /build/target/release/storage-node /build/storage-node

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y libssl3 ca-certificates && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=builder /build/storage-node /app/storage-node
COPY .env.example .env
EXPOSE 50051 8082
CMD ["./storage-node"]
