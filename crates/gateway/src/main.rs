//! Gateway service entry point.
//!
//! Reads configuration from the environment and serves the client-facing HTTP
//! API, proxying metadata and placement to the coordinator over gRPC.

use std::net::SocketAddr;

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

/// Ensure an endpoint has an HTTP scheme so tonic's `Channel::from_shared` works.
fn normalize_endpoint(addr: &str) -> String {
    if addr.starts_with("http://") || addr.starts_with("https://") {
        addr.to_string()
    } else {
        format!("http://{addr}")
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    observability::init()?;

    let gateway_addr = env_or("GATEWAY_ADDR", "0.0.0.0:8080");
    let coordinator_addr = normalize_endpoint(&env_or("COORDINATOR_ADDR", "127.0.0.1:50051"));

    let addr: SocketAddr = gateway_addr.parse()?;

    gateway::run(addr, coordinator_addr).await
}
