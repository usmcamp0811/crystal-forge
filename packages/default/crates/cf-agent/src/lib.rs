//! Crystal Forge deployment agent.
//!
//! # Crate boundary rules
//!
//! - No `sqlx`, `axum`, PostgreSQL, OIDC, JWT, Argon2, or server modules.
//! - Depends on `cf-config` and `cf-protocol`; no direct `cf-server` dependency.
//! - Browser/WASM-incompatible: uses `nix` inotify, filesystem, and blocking I/O.

pub mod deployment;
mod network;
pub mod system_state;
