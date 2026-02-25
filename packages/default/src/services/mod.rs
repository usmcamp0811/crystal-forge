//! Service layer for Crystal Forge.
//!
//! This module provides a thin service layer that orchestrates business logic
//! between handlers and queries. Services represent use-cases and should not
//! contain direct SQL queries.

pub mod systems;
