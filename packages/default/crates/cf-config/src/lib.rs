//! Loads Crystal Forge configuration and prepares process-scoped cache access.
//!
//! This crate provides pure deserialization and loading of Crystal Forge
//! configuration from TOML files and environment variables.
//! The [`cache_credentials`] module owns protected temporary credential files;
//! consumers retain those owners until their cache subprocesses exit.
//!
//! # Crate boundary rules
//!
//! - No `sqlx`, `axum`, `reqwest`, PostgreSQL, OIDC, or server modules.
//! - Only `cf-protocol` is permitted as a Crystal Forge workspace dependency.
//! - Foundational crate; may not depend on `cf-server`, `cf-builder`, or `cf-agent`.

pub mod attic_urls;
pub mod cache_credentials;
pub mod config;
pub mod evaluator_resources;

// Re-export everything from config module at the crate root for convenience.
pub use attic_urls::{AtticUrlError, AtticUrls, resolve_attic_urls};
pub use config::*;
pub use evaluator_resources::{EvaluatorResourceMode, EvaluatorResourcePlan};

// Compile the obsolete server copy in tests so evaluator contracts cannot drift.
#[cfg(test)]
extern crate self as cf_config;
#[cfg(test)]
mod models {
    pub mod builders {
        pub use cf_protocol::builder::{RemoteBuildExecutionStrategy, SourceInputDeliveryMode};
    }
}
#[cfg(test)]
#[path = "../../cf-server/src/config/server.rs"]
mod obsolete_server_config;
