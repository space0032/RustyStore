# RustyStore gateway image.
# Usage: docker build -f crates/gateway/Dockerfile -t rustystore-gateway .

FROM rust:1-slim-bookworm AS builder
WORKDIR /build
RUN apt-get update && apt-get install -y pkg-config libssl-dev protobuf-compiler && rm -rf /var/lib/apt/lists/*
# The workspace has members under crates/, benchmarks/, and tests/, so the
# whole context is needed for Cargo to resolve the workspace.
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/build/target \
    cargo build --release -p gateway --bin gateway && \
    cp /build/target/release/gateway /build/gateway

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y libssl3 ca-certificates curl && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=builder /build/gateway /app/gateway
COPY .env.example .env
EXPOSE 8080
CMD ["./gateway"]
