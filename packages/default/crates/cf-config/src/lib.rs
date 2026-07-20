//! Crystal Forge configuration loading.
//!
//! This crate provides pure deserialization and loading of Crystal Forge
//! configuration from TOML files and environment variables.
//!
//! # Crate boundary rules
//!
//! - No `sqlx`, `axum`, `reqwest`, PostgreSQL, OIDC, or server modules.
//! - Only `cf-protocol` is permitted as a Crystal Forge workspace dependency.
//! - Foundational crate; may not depend on `cf-server`, `cf-builder`, or `cf-agent`.

pub mod config;

// Re-export everything from config module at the crate root for convenience.
pub use config::*;
