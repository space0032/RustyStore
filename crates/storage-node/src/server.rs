//! Storage node server runner.
//! Wires the Tonic gRPC services to a TCP listener.

use crate::StorageNode;
use std::net::SocketAddr;
use std::sync::Arc;

pub struct StorageNodeServer;

impl StorageNodeServer {
    pub fn new() -> Self {
        Self
    }

    pub async fn run(
        addr: SocketAddr,
        node: Arc<StorageNode>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        crate::service::StorageNodeServer::run(addr, node).await
    }
}

impl Default for StorageNodeServer {
    fn default() -> Self {
        Self::new()
    }
}
