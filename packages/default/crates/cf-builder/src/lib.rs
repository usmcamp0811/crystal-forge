//! Crystal Forge remote build worker.
//!
//! # Crate boundary rules
//!
//! - No `sqlx`, PostgreSQL, `axum`, OIDC, JWT, Argon2, or server-internal modules.
//! - Depends on `cf-config` and `cf-protocol`; no direct `cf-server` dependency.
//! - API-only: all server communication goes through the builder API client.

pub mod build;
pub mod builder;
pub mod cache;
pub mod derivations;
pub mod gc_root;
