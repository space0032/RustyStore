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
        let coordinator = Coordinator::new(store, replication_factor, chunk_size_bytes);
        let svc = coordinator.service();

        tonic::transport::Server::builder()
            .add_service(svc.into_server())
            .serve(addr)
            .await?;

        // Background tasks run as long as the Coordinator is alive.
        // The `Coordinator` struct holds the JoinHandles; dropping it
        // would abort them, but since `serve()` runs forever, they
        // continue for the process lifetime.
        drop(coordinator);
        Ok(())
    }
}
