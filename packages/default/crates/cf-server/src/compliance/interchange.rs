//! Frozen CF-XCCDF v0.1 identifiers and bounded-input limits.
//!
//! The implementation lives in the database-free `cf-compliance` crate so that
//! offline tools share the same frozen contract. This module re-exports it
//! unchanged for existing `crate::compliance::interchange` callers.

pub use cf_compliance::interchange::*;
