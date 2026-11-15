//! Gateway application state and router.

use tokio::sync::Mutex;

use coordinator::CoordinatorClient;

pub struct AppState {
    pub coordinator_addr: String,
    pub coordinator_client: Mutex<Option<CoordinatorClient>>,
    /// Maximum accepted upload size in bytes; `0` means unlimited.
    pub max_upload_bytes: u64,
}

impl AppState {
    pub fn new(coordinator_addr: &str) -> Self {
        Self {
            coordinator_addr: coordinator_addr.to_string(),
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
            let client = CoordinatorClient::connect(&self.coordinator_addr)
                .await
                .map_err(|e| crate::error::GatewayError::Grpc(e.to_string()))?;
            *guard = Some(client);
        }
        Ok(guard.as_ref().unwrap().clone())
    }
}

impl Clone for AppState {
    fn clone(&self) -> Self {
        Self {
            coordinator_addr: self.coordinator_addr.clone(),
            coordinator_client: Mutex::new(None),
            max_upload_bytes: self.max_upload_bytes,
        }
    }
}
