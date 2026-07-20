//! Crystal Forge wire protocol types.
//!
//! This crate contains serializable types shared between the Crystal Forge
//! server, remote builder, and deployment agent. It has no database, HTTP
//! server, or server-internal dependencies.
//!
//! # Crate boundary rules
//!
//! - No `sqlx`, `axum`, `reqwest`, PostgreSQL, OIDC, or server module imports.
//! - No process-specific entrypoint logic.
//! - Types may derive `Serialize` and `Deserialize` but not `sqlx::FromRow`.
//! - Foundational; may not depend on `cf-config`, `cf-server`, `cf-builder`,
//!   or `cf-agent`.

pub mod agent;
pub mod builder;
pub mod cache;
pub(crate) mod network;
