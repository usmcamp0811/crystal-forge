//! Reusable test utilities for Crystal Forge.
//!
//! This module provides:
//!
//! - **Builders** — type-safe builder pattern for constructing domain types with
//!   sensible defaults.  Every builder produces a valid instance with zero
//!   customisation; individual fields can be overridden as needed.
//!
//! - **Assertions** — thin wrappers around common assertion patterns to keep
//!   tests concise and intention-revealing.
//!
//! - **Crypto helpers** — deterministic ed25519 key generation for tests that
//!   need `PublicKey` / `System` instances without touching real key material.
//!
//! # Quick Start
//!
//! ```rust,ignore
//! use crystal_forge::test_utils::builders::*;
//!
//! let derivation = DerivationBuilder::new().name("hello").build();
//! let commit     = CommitBuilder::new().hash("abc123").build();
//! let flake      = FlakeBuilder::new().name("my-flake").build();
//! let state      = SystemStateBuilder::new().hostname("web-1").build();
//! ```

pub mod assertions;
pub mod builders;
pub mod crypto;
