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
        let coordinator = Coordinator::new(
            store,
            replication_factor,
            chunk_size_bytes,
            crate::gc::GcConfig::from_env()?,
        );
        let svc = coordinator.service();

        tonic::transport::Server::builder()
            .add_service(svc.into_server())
            .serve_with_shutdown(addr, shutdown)
            .await?;

        // Background tasks run as long as the Coordinator is alive.
        // The `Coordinator` struct holds the JoinHandles; dropping it
        // matches the process lifetime.
        drop(coordinator);
        Ok(())
    }
}
