//! Shared types, errors, and configuration primitives used across the
//! RustyStore workspace.
//!
//! This crate is a leaf dependency: it does not depend on any other
//! application crate, keeping the dependency graph acyclic.

pub mod config;
pub mod error;
pub mod request_id;
pub mod shutdown;
pub mod tls;
pub mod types;

pub use error::RustyStoreError;
