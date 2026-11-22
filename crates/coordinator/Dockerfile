# RustyStore coordinator image.
# Usage: docker build -f crates/coordinator/Dockerfile -t rustystore-coordinator .

FROM rust:1-slim-bookworm AS builder
WORKDIR /build
RUN apt-get update && apt-get install -y pkg-config libssl-dev protobuf-compiler && rm -rf /var/lib/apt/lists/*
# The workspace has members under crates/, benchmarks/, and tests/, so the
# whole context is needed for Cargo to resolve the workspace.
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/build/target \
    cargo build --release -p coordinator --bin coordinator && \
    cp /build/target/release/coordinator /build/coordinator

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y libssl3 ca-certificates curl && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=builder /build/coordinator /app/coordinator
COPY .env.example .env
EXPOSE 50051 8081
CMD ["./coordinator"]
