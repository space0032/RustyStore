//! Process shutdown signalling.
//!
//! Services should shut down gracefully on both Ctrl+C (SIGINT) and SIGTERM.
//! Docker and Kubernetes send SIGTERM, so listening only for Ctrl+C leaves
//! containers to be killed abruptly.

/// Resolves when the process receives a shutdown signal.
pub async fn signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};

        let mut sigterm =
            signal(SignalKind::terminate()).expect("failed to install SIGTERM handler");
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = sigterm.recv() => {}
        }
    }

    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}
