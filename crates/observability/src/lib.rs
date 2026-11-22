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

pub static COORDINATOR_IS_LEADER: Lazy<IntGauge> = Lazy::new(|| {
    IntGauge::new(
        "rustystore_coordinator_is_leader",
        "Whether this coordinator holds the background-work lease (1 = leader)",
    )
    .unwrap()
});

pub static STORAGE_CHUNKS_STORED: Lazy<IntCounter> = Lazy::new(|| {
    IntCounter::new(
        "rustystore_storage_chunks_stored_total",
        "Chunks stored by this storage node",
    )
    .unwrap()
});

pub static STORAGE_BYTES_STORED: Lazy<IntCounter> = Lazy::new(|| {
    IntCounter::new(
        "rustystore_storage_bytes_stored_total",
        "Chunk bytes stored by this storage node",
    )
    .unwrap()
});

pub static STORAGE_CHUNKS_DELETED: Lazy<IntCounter> = Lazy::new(|| {
    IntCounter::new(
        "rustystore_storage_chunks_deleted_total",
        "Chunks deleted by this storage node",
    )
    .unwrap()
});

pub static ORPHAN_CHUNKS_DELETED: Lazy<IntCounter> = Lazy::new(|| {
    IntCounter::new(
        "rustystore_orphan_chunks_deleted_total",
        "Unreferenced chunk bytes reclaimed by the coordinator",
    )
    .unwrap()
});

pub static ORPHAN_BYTES_DELETED: Lazy<IntCounter> = Lazy::new(|| {
    IntCounter::new(
        "rustystore_orphan_bytes_deleted_total",
        "Unreferenced chunk bytes (size) reclaimed by the coordinator",
    )
    .unwrap()
});

pub static STALE_UPLOADS_REAPED: Lazy<IntCounter> = Lazy::new(|| {
    IntCounter::new(
        "rustystore_stale_uploads_reaped_total",
        "Abandoned upload sessions/files removed by the coordinator",
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
    register(UPLOADED_BYTES.clone())?;
    register(DOWNLOADED_BYTES.clone())?;
    register(UPLOAD_FAILURES.clone())?;
    register(ACTIVE_NODES.clone())?;
    register(UNDER_REPLICATED_CHUNKS.clone())?;
    register(COORDINATOR_IS_LEADER.clone())?;
    register(STORAGE_CHUNKS_STORED.clone())?;
    register(STORAGE_BYTES_STORED.clone())?;
    register(STORAGE_CHUNKS_DELETED.clone())?;
    register(ORPHAN_CHUNKS_DELETED.clone())?;
    register(ORPHAN_BYTES_DELETED.clone())?;
    register(STALE_UPLOADS_REAPED.clone())?;
    Ok(())
}

/// Register a collector, treating re-registration as success so that
/// `register_metrics()` is idempotent (safe to call from multiple entry points
/// or tests in the same process).
fn register<C>(collector: C) -> Result<(), prometheus::Error>
where
    C: prometheus::core::Collector + 'static,
{
    match REGISTRY.register(Box::new(collector)) {
        Ok(()) => Ok(()),
        Err(prometheus::Error::AlreadyReg) => Ok(()),
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metrics_render_contains_counters_and_is_idempotent() {
        register_metrics().unwrap();
        register_metrics().unwrap();

        UPLOADED_BYTES.inc();
        DOWNLOADED_BYTES.inc();
        ACTIVE_NODES.set(1);
        COORDINATOR_IS_LEADER.set(1);
        STORAGE_CHUNKS_STORED.inc();
        STORAGE_BYTES_STORED.inc_by(16);
        STORAGE_CHUNKS_DELETED.inc();
        ORPHAN_CHUNKS_DELETED.inc();
        ORPHAN_BYTES_DELETED.inc_by(8);
        STALE_UPLOADS_REAPED.inc();

        let text = prometheus_metrics();
        assert!(text.contains("rustystore_uploaded_bytes_total"), "{text}");
        assert!(text.contains("rustystore_downloaded_bytes_total"), "{text}");
        assert!(text.contains("rustystore_active_storage_nodes"), "{text}");
        assert!(text.contains("rustystore_coordinator_is_leader"), "{text}");
        assert!(
            text.contains("rustystore_storage_chunks_stored_total"),
            "{text}"
        );
        assert!(
            text.contains("rustystore_storage_bytes_stored_total"),
            "{text}"
        );
        assert!(
            text.contains("rustystore_storage_chunks_deleted_total"),
            "{text}"
        );
        assert!(
            text.contains("rustystore_orphan_chunks_deleted_total"),
            "{text}"
        );
        assert!(
            text.contains("rustystore_orphan_bytes_deleted_total"),
            "{text}"
        );
        assert!(
            text.contains("rustystore_stale_uploads_reaped_total"),
            "{text}"
        );
    }
}
