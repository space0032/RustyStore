//! Gateway service entry point.
//!
//! Reads configuration from the environment and serves the client-facing HTTP
//! API, proxying metadata and placement to the coordinator over gRPC.

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

    let addr = common::config::parse_addr("GATEWAY_ADDR", "0.0.0.0:8080")?;
    let coordinator_raw = common::config::env_string("COORDINATOR_ADDR", "127.0.0.1:50051");
    common::config::require_non_empty("COORDINATOR_ADDR", &coordinator_raw)?;
    let coordinator_addr = normalize_endpoint(&coordinator_raw);

    gateway::run(addr, coordinator_addr).await
}
