//! Service layer for Crystal Forge.
//!
//! This module provides a thin service layer that orchestrates business logic
//! between handlers and queries. Services represent use-cases and should not
//! contain direct SQL queries.

pub mod approval_policy;
pub mod canary_rollout;
pub mod composite_enforcement;
pub mod cve_policy_gate;
pub mod cve_scans;
pub mod cve_threshold_policy;
pub mod hardening_scans;
pub mod poam;
pub mod systems;
pub mod time_window_policy;
