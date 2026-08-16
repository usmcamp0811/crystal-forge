//! Canonicalization and semantic-digest contract.
//!
//! The implementation lives in the database-free `cf-compliance` crate so that
//! offline tools reuse the exact same `cf-model-json-1` semantics. This module
//! re-exports it unchanged for existing `crate::compliance::canonical` callers.

pub use cf_compliance::canonical::*;
