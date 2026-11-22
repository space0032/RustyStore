//! Gateway application state and router.

use std::time::Duration;

use tokio::sync::Mutex;
use tracing::warn;

use coordinator::CoordinatorClient;

pub struct AppState {
    pub coordinator_addrs: Vec<String>,
    pub coordinator_client: Mutex<Option<CoordinatorClient>>,
    /// Maximum accepted upload size in bytes; `0` means unlimited.
    pub max_upload_bytes: u64,
}

impl AppState {
    pub fn new(coordinator_addr: &str) -> Self {
        Self::with_coordinator_addrs(vec![coordinator_addr.to_string()])
    }

    pub fn with_coordinator_addrs(coordinator_addrs: Vec<String>) -> Self {
        Self {
            coordinator_addrs,
            coordinator_client: Mutex::new(None),
            max_upload_bytes: 0,
        }
    }

    /// Override the upload size limit (used in tests and custom wiring).
    pub fn with_max_upload_bytes(mut self, max_upload_bytes: u64) -> Self {
        self.max_upload_bytes = max_upload_bytes;
        self
    }

    pub async fn coord_client(&self) -> Result<CoordinatorClient, crate::error::GatewayError> {
        let mut guard = self.coordinator_client.lock().await;
        if guard.is_none() {
            let client = CoordinatorClient::connect_many(&self.coordinator_addrs)
                .await
                .map_err(|e| crate::error::GatewayError::Grpc(e.to_string()))?;
            *guard = Some(client);
        }
        Ok(guard.as_ref().unwrap().clone())
    }

    /// Refresh the coordinator pool from the cluster registry.
    ///
    /// Returns how many new endpoints were added. Discovery is best-effort:
    /// failures leave the existing bootstrap pool untouched.
    pub async fn refresh_coordinators(&self) -> usize {
        let existing = { self.coordinator_client.lock().await.clone() };
        let mut client = match existing {
            Some(client) => client,
            None => match CoordinatorClient::connect_many(&self.coordinator_addrs).await {
                Ok(client) => client,
                Err(e) => {
                    warn!("coordinator discovery unavailable: {e}");
                    return 0;
                }
            },
        };

        let discovered =
            match tokio::time::timeout(Duration::from_secs(5), client.list_coordinators()).await {
                Ok(Ok(coordinators)) => coordinators,
                Ok(Err(e)) => {
                    warn!("coordinator discovery RPC failed: {e}");
                    return 0;
                }
                Err(_) => {
                    warn!("coordinator discovery RPC timed out");
                    return 0;
                }
            };
        let discovered: Vec<String> = discovered
            .into_iter()
            .map(|(_, addr)| crate::grpc_endpoint(&addr))
            .collect();

        let mut guard = self.coordinator_client.lock().await;
        if let Some(current) = guard.as_mut() {
            let before = current.addrs().len();
            current.merge_addrs(discovered);
            current.addrs().len().saturating_sub(before)
        } else {
            let before = client.addrs().len();
            client.merge_addrs(discovered);
            let added = client.addrs().len().saturating_sub(before);
            *guard = Some(client);
            added
        }
    }
}

impl Clone for AppState {
    fn clone(&self) -> Self {
        Self {
            coordinator_addrs: self.coordinator_addrs.clone(),
            coordinator_client: Mutex::new(None),
            max_upload_bytes: self.max_upload_bytes,
        }
    }
}
