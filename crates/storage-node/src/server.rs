//! Storage node server runner (stub).

pub struct StorageNodeServer;

impl StorageNodeServer {
    pub async fn run(_addr: std::net::SocketAddr) -> Result<(), Box<dyn std::error::Error>> {
        Err("not implemented in Phase 1 stub".into())
    }
}
