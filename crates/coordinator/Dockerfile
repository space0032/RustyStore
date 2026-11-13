# RustyStore coordinator image.
# Usage: docker build -f crates/coordinator/Dockerfile -t rustystore-coordinator .

FROM rust:1.82-slim AS builder
WORKDIR /build
RUN apt-get update && apt-get install -y pkg-config libssl-dev protobuf-compiler && rm -rf /var/lib/apt/lists/*
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY crates ./crates
COPY proto ./proto
COPY migrations ./migrations
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/build/target \
    cargo build --release -p coordinator --bin coordinator && \
    cp /build/target/release/coordinator /build/coordinator

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y libssl3 ca-certificates && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=builder /build/coordinator /app/coordinator
COPY .env.example .env
EXPOSE 50051
CMD ["./coordinator"]
