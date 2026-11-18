//! Gateway service entry point.
//!
//! Reads configuration from the environment and serves the client-facing HTTP
//! API, proxying metadata and placement to the coordinator over gRPC.

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    observability::init()?;

    let addr = common::config::parse_addr("GATEWAY_ADDR", "0.0.0.0:8080")?;
    let coordinator_addrs = gateway::coordinator_addrs_from_env()?;

    gateway::run(addr, coordinator_addrs).await
}
