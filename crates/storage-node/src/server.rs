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

    /// Serve until `shutdown` resolves, then drain in-flight requests.
    pub async fn run_with_shutdown(
        addr: SocketAddr,
        node: Arc<StorageNode>,
        shutdown: impl std::future::Future<Output = ()> + Send + 'static,
    ) -> Result<(), Box<dyn std::error::Error>> {
        crate::service::StorageNodeServer::run_with_shutdown(addr, node, shutdown).await
    }

    /// Serve on an already-bound listener until `shutdown` resolves. Useful for
    /// embedding and for tests that need an ephemeral port.
    pub async fn serve_with_listener(
        listener: tokio::net::TcpListener,
        node: Arc<StorageNode>,
        shutdown: impl std::future::Future<Output = ()> + Send + 'static,
    ) -> Result<(), Box<dyn std::error::Error>> {
        crate::service::StorageNodeServer::serve_with_listener_shutdown(listener, node, shutdown)
            .await
    }
}

impl Default for StorageNodeServer {
    fn default() -> Self {
        Self::new()
    }
}
