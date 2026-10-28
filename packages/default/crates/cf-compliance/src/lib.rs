//! Crystal Forge compliance interchange primitives.
//!
//! This crate holds the database-free half of the compliance interchange
//! layer: frozen CF-XCCDF v0.1 identifiers and input limits, the
//! `cf-model-json-1` canonicalization and semantic-digest contract, the typed
//! canonical digest DTOs, the secure XCCDF/ZIP reading stack, and the policy
//! validation that import and export boundaries share: typed composite policy
//! configuration ([`policy_config`]), custom-check normalization
//! ([`custom_check`]), and time-window rules ([`time_window`]).
//!
//! It deliberately depends on no database, HTTP, or async runtime crate so
//! that offline tools (for example the `cf-nixos-module` generator) can reuse
//! the exact same interchange semantics as the server. `cf-server` re-exports
//! every item here under `crystal_forge::compliance::*` and
//! `crystal_forge::models::*`, so this split is not visible to existing server
//! call sites.
//!
//! # Nix syntax checking
//!
//! Full policy validation ([`policy_config::validate_policy_type_config`],
//! [`xccdf::importer::validate_cf_native_document`]) runs one bounded
//! `nix-instantiate --parse` subprocess to syntax-check custom expressions, and
//! therefore needs a `nix-instantiate` executable on `PATH`. Server import and
//! publication boundaries and the standalone generator use these. Persisted
//! runtime configuration can instead use
//! [`policy_config::deserialize_policy_type_config`], which performs structural
//! validation without starting the parser.

pub mod canonical;
pub mod custom_check;
pub mod digest;
pub mod effective_set;
pub mod interchange;
pub mod policy_config;
pub mod policy_document;
pub mod time_window;
pub mod xccdf;
