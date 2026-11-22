//! Environment-based configuration.

use std::net::SocketAddr;
use std::str::FromStr;
use thiserror::Error;

/// A configuration value that could not be read or validated at startup.
#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum ConfigError {
    #[error("{key} has an invalid value {value:?}")]
    InvalidValue { key: String, value: String },

    #[error("{key} must be a valid socket address, got {value:?}")]
    InvalidAddr { key: String, value: String },

    #[error("{key} must be greater than zero, got {value}")]
    NotPositive { key: String, value: i64 },

    #[error("{key} must not be empty")]
    Empty { key: String },
}

/// Read an environment variable, falling back to `default`.
pub fn env_string(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

/// Parse a pure value with a key for error context.
pub fn parse_value<T: FromStr>(key: &str, raw: &str) -> Result<T, ConfigError> {
    raw.trim().parse().map_err(|_| ConfigError::InvalidValue {
        key: key.to_string(),
        value: raw.to_string(),
    })
}

/// Read and parse an environment variable, falling back to `default`.
///
/// Unlike silently defaulting on a typo, an unparsable value fails startup.
pub fn parse_env<T: FromStr>(key: &str, default: T) -> Result<T, ConfigError> {
    match std::env::var(key) {
        Ok(raw) => parse_value(key, &raw),
        Err(_) => Ok(default),
    }
}

/// Parse a pure socket address with a key for error context.
pub fn parse_addr_value(key: &str, raw: &str) -> Result<SocketAddr, ConfigError> {
    raw.trim().parse().map_err(|_| ConfigError::InvalidAddr {
        key: key.to_string(),
        value: raw.to_string(),
    })
}

/// Read and parse a socket address, falling back to `default`.
pub fn parse_addr(key: &str, default: &str) -> Result<SocketAddr, ConfigError> {
    parse_addr_value(key, &env_string(key, default))
}

/// Require a value to be strictly positive.
pub fn require_positive(key: &str, value: i64) -> Result<i64, ConfigError> {
    if value > 0 {
        Ok(value)
    } else {
        Err(ConfigError::NotPositive {
            key: key.to_string(),
            value,
        })
    }
}

/// Require a value to be non-empty (after trimming whitespace).
pub fn require_non_empty(key: &str, value: &str) -> Result<(), ConfigError> {
    if value.trim().is_empty() {
        Err(ConfigError::Empty {
            key: key.to_string(),
        })
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_value_accepts_valid_integer() {
        assert_eq!(parse_value::<i32>("REPLICATION_FACTOR", " 5 "), Ok(5));
    }

    #[test]
    fn parse_value_rejects_garbage_with_key_context() {
        let err = parse_value::<i32>("REPLICATION_FACTOR", "three").unwrap_err();
        assert_eq!(
            err,
            ConfigError::InvalidValue {
                key: "REPLICATION_FACTOR".to_string(),
                value: "three".to_string(),
            }
        );
        assert!(err.to_string().contains("REPLICATION_FACTOR"));
    }

    #[test]
    fn parse_addr_value_accepts_valid_address() {
        let addr = parse_addr_value("GATEWAY_ADDR", "127.0.0.1:8080").unwrap();
        assert_eq!(addr.port(), 8080);
    }

    #[test]
    fn parse_addr_value_rejects_garbage() {
        let err = parse_addr_value("GATEWAY_ADDR", "not-an-address").unwrap_err();
        assert!(matches!(err, ConfigError::InvalidAddr { .. }));
    }

    #[test]
    fn require_positive_rejects_zero_and_negatives() {
        assert_eq!(require_positive("CHUNK_SIZE_BYTES", 1), Ok(1));
        assert!(matches!(
            require_positive("CHUNK_SIZE_BYTES", 0),
            Err(ConfigError::NotPositive { .. })
        ));
        assert!(require_positive("CHUNK_SIZE_BYTES", -1).is_err());
    }

    #[test]
    fn require_non_empty_rejects_blank() {
        assert!(require_non_empty("STORAGE_PATH", "/data/chunks").is_ok());
        assert!(matches!(
            require_non_empty("STORAGE_PATH", "   "),
            Err(ConfigError::Empty { .. })
        ));
    }
}
