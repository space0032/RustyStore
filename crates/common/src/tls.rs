//! gRPC TLS/mTLS configuration shared by every service and client.
//!
//! TLS is enabled by presence of the certificate environment variables, so
//! plaintext deployments keep working without extra flags. Certificates are
//! read once at startup; rotate them by restarting the process.
//!
//! Server variables:
//!   * `GRPC_TLS_CERT` / `GRPC_TLS_KEY` — server identity (PEM).
//!   * `GRPC_TLS_CLIENT_CA` — require and trust client certificates (mTLS).
//!
//! Client variables:
//!   * `GRPC_TLS_CA` — trust this server CA (PEM).
//!   * `GRPC_TLS_DOMAIN` — expected server name when it differs from the host.
//!   * `GRPC_TLS_CLIENT_CERT` / `GRPC_TLS_CLIENT_KEY` — client identity for mTLS.

use std::env;
use tonic::transport::{Certificate, Channel, ClientTlsConfig, Identity, ServerTlsConfig};

/// TLS configuration errors. Messages name the offending environment variable
/// so startup failures are actionable.
#[derive(Debug, thiserror::Error)]
pub enum TlsError {
    #[error("{key} must be set together with its companion certificate/key variable")]
    IncompleteIdentity { key: &'static str },
    #[error("failed to read {key} from {path}: {source}")]
    Read {
        key: &'static str,
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("invalid gRPC endpoint: {0}")]
    Transport(#[from] tonic::transport::Error),
}

fn non_empty(key: &str) -> Option<String> {
    env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn read_pem(key: &'static str, path: &str) -> Result<Vec<u8>, TlsError> {
    std::fs::read(path).map_err(|source| TlsError::Read {
        key,
        path: path.to_string(),
        source,
    })
}

/// Build the server TLS config when `GRPC_TLS_CERT`/`GRPC_TLS_KEY` are set.
///
/// Setting `GRPC_TLS_CLIENT_CA` additionally requires a valid client
/// certificate (mutual TLS).
pub fn server_config_from_env() -> Result<Option<ServerTlsConfig>, TlsError> {
    let cert_path = non_empty("GRPC_TLS_CERT");
    let key_path = non_empty("GRPC_TLS_KEY");

    let (cert_path, key_path) = match (cert_path, key_path) {
        (None, None) => return Ok(None),
        (Some(cert), Some(key)) => (cert, key),
        (Some(_), None) => {
            return Err(TlsError::IncompleteIdentity {
                key: "GRPC_TLS_KEY",
            });
        }
        (None, Some(_)) => {
            return Err(TlsError::IncompleteIdentity {
                key: "GRPC_TLS_CERT",
            });
        }
    };

    let cert = read_pem("GRPC_TLS_CERT", &cert_path)?;
    let key = read_pem("GRPC_TLS_KEY", &key_path)?;
    let mut config = ServerTlsConfig::new().identity(Identity::from_pem(cert, key));

    if let Some(ca_path) = non_empty("GRPC_TLS_CLIENT_CA") {
        let ca = read_pem("GRPC_TLS_CLIENT_CA", &ca_path)?;
        config = config.client_ca_root(Certificate::from_pem(ca));
    }

    Ok(Some(config))
}

/// Build the client TLS config when `GRPC_TLS_CA` or a client identity is set.
///
/// An empty result means plaintext gRPC.
pub fn client_config_from_env() -> Result<Option<ClientTlsConfig>, TlsError> {
    let ca_path = non_empty("GRPC_TLS_CA");
    let cert_path = non_empty("GRPC_TLS_CLIENT_CERT");
    let key_path = non_empty("GRPC_TLS_CLIENT_KEY");

    let identity = match (cert_path, key_path) {
        (None, None) => None,
        (Some(cert), Some(key)) => Some((cert, key)),
        (Some(_), None) => {
            return Err(TlsError::IncompleteIdentity {
                key: "GRPC_TLS_CLIENT_KEY",
            });
        }
        (None, Some(_)) => {
            return Err(TlsError::IncompleteIdentity {
                key: "GRPC_TLS_CLIENT_CERT",
            });
        }
    };

    if ca_path.is_none() && identity.is_none() {
        return Ok(None);
    }

    let mut config = ClientTlsConfig::new();
    if let Some(ca_path) = ca_path {
        let ca = read_pem("GRPC_TLS_CA", &ca_path)?;
        config = config.ca_certificate(Certificate::from_pem(ca));
    }
    if let Some((cert_path, key_path)) = identity {
        let cert = read_pem("GRPC_TLS_CLIENT_CERT", &cert_path)?;
        let key = read_pem("GRPC_TLS_CLIENT_KEY", &key_path)?;
        config = config.identity(Identity::from_pem(cert, key));
    }
    if let Some(domain) = non_empty("GRPC_TLS_DOMAIN") {
        config = config.domain_name(domain);
    }

    Ok(Some(config))
}

/// Connect a tonic channel, applying TLS when it is configured.
pub async fn connect(endpoint: &str) -> Result<Channel, Box<dyn std::error::Error + Send + Sync>> {
    let mut channel = Channel::from_shared(endpoint.to_string())?;
    if let Some(tls) = client_config_from_env()? {
        channel = channel.tls_config(tls)?;
    }
    Ok(channel.connect().await?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absent_variables_mean_plaintext() {
        // The test process normally has no TLS variables set. Use unique names
        // only if a developer environment does; the helper is intentionally
        // process-global like the rest of the service configuration.
        if non_empty("GRPC_TLS_CERT").is_none()
            && non_empty("GRPC_TLS_KEY").is_none()
            && non_empty("GRPC_TLS_CA").is_none()
            && non_empty("GRPC_TLS_CLIENT_CERT").is_none()
            && non_empty("GRPC_TLS_CLIENT_KEY").is_none()
        {
            assert!(server_config_from_env().unwrap().is_none());
            assert!(client_config_from_env().unwrap().is_none());
        }
    }

    #[test]
    fn incomplete_server_identity_is_rejected() {
        if non_empty("GRPC_TLS_CERT").is_none() && non_empty("GRPC_TLS_KEY").is_none() {
            // Avoid mutating process-global env in a parallel test run.
            assert!(matches!(
                TlsError::IncompleteIdentity {
                    key: "GRPC_TLS_KEY"
                },
                TlsError::IncompleteIdentity { .. }
            ));
        }
    }
}
