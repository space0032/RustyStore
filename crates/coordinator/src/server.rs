//! Coordinator server runner.
//! Wires the Tonic service + background tasks to a TCP listener.

use crate::Coordinator;
use metadata::MetadataStore;
use std::net::SocketAddr;
use std::sync::Arc;

pub struct CoordinatorServer;

impl CoordinatorServer {
    pub async fn run(
        addr: SocketAddr,
        store: Arc<MetadataStore>,
        replication_factor: i32,
        chunk_size_bytes: i64,
    ) -> Result<(), Box<dyn std::error::Error>> {
        Self::run_with_shutdown(
            addr,
            store,
            replication_factor,
            chunk_size_bytes,
            common::shutdown::signal(),
        )
        .await
    }

    /// Serve until `shutdown` resolves, then stop accepting new requests and
    /// let in-flight requests finish.
    pub async fn run_with_shutdown(
        addr: SocketAddr,
        store: Arc<MetadataStore>,
        replication_factor: i32,
        chunk_size_bytes: i64,
        shutdown: impl std::future::Future<Output = ()> + Send + 'static,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let listener = tokio::net::TcpListener::bind(addr).await?;
        Self::serve_with_listener(
            listener,
            store,
            replication_factor,
            chunk_size_bytes,
            shutdown,
        )
        .await
    }

    /// Serve on an already-bound listener until `shutdown` resolves. Useful for
    /// embedding and for tests that need an ephemeral port.
    pub async fn serve_with_listener(
        listener: tokio::net::TcpListener,
        store: Arc<MetadataStore>,
        replication_factor: i32,
        chunk_size_bytes: i64,
        shutdown: impl std::future::Future<Output = ()> + Send + 'static,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let recovery_bandwidth_bytes_per_sec = std::env::var("RECOVERY_BANDWIDTH_BYTES_PER_SEC")
            .ok()
            .map(|raw| common::config::parse_value::<u64>("RECOVERY_BANDWIDTH_BYTES_PER_SEC", &raw))
            .transpose()?
            .filter(|rate| *rate > 0);
        let recovery_aggregate_bytes_per_sec = std::env::var("RECOVERY_AGGREGATE_BYTES_PER_SEC")
            .ok()
            .map(|raw| common::config::parse_value::<u64>("RECOVERY_AGGREGATE_BYTES_PER_SEC", &raw))
            .transpose()?
            .filter(|rate| *rate > 0);
        let recovery_priority_aging_secs: i64 =
            common::config::parse_env("RECOVERY_PRIORITY_AGING_SECS", 60)?;
        if recovery_priority_aging_secs < 0 {
            return Err("RECOVERY_PRIORITY_AGING_SECS must not be negative".into());
        }

        let coordinator = Coordinator::new(
            store,
            replication_factor,
            chunk_size_bytes,
            crate::gc::GcConfig::from_env()?,
            recovery_bandwidth_bytes_per_sec,
            recovery_aggregate_bytes_per_sec,
            recovery_priority_aging_secs,
        );
        let svc = coordinator.service();

        let mut builder = tonic::transport::Server::builder();
        if let Some(tls) = common::tls::server_config_from_env()? {
            builder = builder.tls_config(tls)?;
        }
        builder
            .add_service(svc.into_server())
            .serve_with_incoming_shutdown(
                tokio_stream::wrappers::TcpListenerStream::new(listener),
                shutdown,
            )
            .await?;

        // Background tasks run as long as the Coordinator is alive.
        // The `Coordinator` struct holds the JoinHandles; dropping it
        // matches the process lifetime.
        drop(coordinator);
        Ok(())
    }
}
