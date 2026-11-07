//! Coordinator server runner.
//! Wires the Tonic service to a TCP listener.

use crate::service::CoordinatorService;
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
        let svc = CoordinatorService::new(store, replication_factor, chunk_size_bytes);
        tonic::transport::Server::builder()
            .add_service(svc.into_server())
            .serve(addr)
            .await?;
        Ok(())
    }
}
