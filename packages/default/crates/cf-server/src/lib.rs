//! Provides Crystal Forge server APIs, domain models, persistence, and
//! services.
//!
//! The server crate owns authorization, durable state, policy enforcement, and
//! coordination with agents and builders. Public modules expose these
//! server-side boundaries to workspace binaries and integration tests.

pub mod api;
pub mod auth;
pub mod builder;
pub mod compliance;
pub mod config;
pub mod deployment;
pub mod derivations;
pub mod fixtures;
pub mod flake;
pub mod handlers;
pub mod hardening;
pub mod log;
pub mod models;
pub mod nixos_options_metadata;
pub mod queries;
pub mod queue;
pub mod security;
pub mod server;
pub mod services;
pub mod tasks;
pub mod vulnix;

#[cfg(test)]
pub mod test_utils;
