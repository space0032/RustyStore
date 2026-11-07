//! Observability: tracing + Prometheus metrics.
//!
//! Initializes a JSON-formatted tracing subscriber (filtered by RUST_LOG) and
//! exposes Prometheus counters/gauges for upload/download bytes, failures,
//! node health, and replication stats.

use once_cell::sync::Lazy;
use prometheus::{Encoder, IntCounter, IntGauge, Registry, TextEncoder};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

pub static REGISTRY: Lazy<Registry> = Lazy::new(Registry::new);

pub static UPLOADED_BYTES: Lazy<IntCounter> = Lazy::new(|| {
    IntCounter::new("rustystore_uploaded_bytes_total", "Total uploaded bytes").unwrap()
});

pub static DOWNLOADED_BYTES: Lazy<IntCounter> = Lazy::new(|| {
    IntCounter::new(
        "rustystore_downloaded_bytes_total",
        "Total downloaded bytes",
    )
    .unwrap()
});

pub static UPLOAD_FAILURES: Lazy<IntCounter> = Lazy::new(|| {
    IntCounter::new("rustystore_upload_failures_total", "Total upload failures").unwrap()
});

pub static ACTIVE_NODES: Lazy<IntGauge> =
    Lazy::new(|| IntGauge::new("rustystore_active_storage_nodes", "Active storage nodes").unwrap());

pub static UNDER_REPLICATED_CHUNKS: Lazy<IntGauge> = Lazy::new(|| {
    IntGauge::new(
        "rustystore_under_replicated_chunks",
        "Under-replicated chunks",
    )
    .unwrap()
});

pub fn init() -> Result<(), Box<dyn std::error::Error>> {
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    let fmt_layer = tracing_subscriber::fmt::layer().json();
    tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt_layer)
        .init();
    Ok(())
}

pub fn prometheus_metrics() -> String {
    let mut buffer = Vec::new();
    let encoder = TextEncoder::new();
    let metric_families = REGISTRY.gather();
    encoder.encode(&metric_families, &mut buffer).unwrap();
    String::from_utf8(buffer).unwrap()
}

pub fn register_metrics() -> Result<(), Box<dyn std::error::Error>> {
    REGISTRY.register(Box::new(UPLOADED_BYTES.clone()))?;
    REGISTRY.register(Box::new(DOWNLOADED_BYTES.clone()))?;
    REGISTRY.register(Box::new(UPLOAD_FAILURES.clone()))?;
    REGISTRY.register(Box::new(ACTIVE_NODES.clone()))?;
    REGISTRY.register(Box::new(UNDER_REPLICATED_CHUNKS.clone()))?;
    Ok(())
}
