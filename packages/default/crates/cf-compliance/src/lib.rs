//! Crystal Forge compliance interchange primitives.
//!
//! This crate holds the database-free half of the compliance interchange
//! layer: frozen CF-XCCDF v0.1 identifiers and input limits, the
//! `cf-model-json-1` canonicalization and semantic-digest contract, the typed
//! canonical digest DTOs, and the secure XCCDF/ZIP reading stack.
//!
//! It deliberately depends on no database, HTTP, or async runtime crate so
//! that offline tools (for example the `cf-nixos-module` generator) can reuse
//! the exact same interchange semantics as the server. `cf-server` re-exports
//! every item here under `crystal_forge::compliance::*`, so this split is not
//! visible to existing server call sites.

pub mod canonical;
pub mod digest;
pub mod interchange;
pub mod policy_document;
pub mod xccdf;
