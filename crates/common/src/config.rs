//! Environment-based configuration.

use serde::Deserialize;
use std::time::Duration;

#[derive(Clone, Debug, Deserialize)]
pub struct Config {
    pub coordinator_addr: String,
    pub gateway_addr: String,
    pub database_url: String,
    pub chunk_size_bytes: u64,
    pub replication_factor: i32,
    pub heartbeat_interval_ms: u64,
    pub lease_ttl_ms: u64,
    pub storage_path: String,
    pub node_addr: String,
    pub prometheus_addr: String,
}

impl Config {
    pub fn from_env() -> Result<Self, envy::Error> {
        envy::from_env::<Config>()
    }

    pub fn heartbeat_interval(&self) -> Duration {
        Duration::from_millis(self.heartbeat_interval_ms)
    }

    pub fn lease_ttl(&self) -> Duration {
        Duration::from_millis(self.lease_ttl_ms)
    }
}
