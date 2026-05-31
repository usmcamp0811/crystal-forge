//! Hardening scan models.
//!
//! Re-exports from the hardening module for consistency with other models.

pub use crate::hardening::types::{
    DirectiveDetail, FleetHardeningSummary, HardeningJustification, HardeningScan,
    JustificationCategory, RiskLevel, ScanStatus, ServiceHardeningResult, SystemHardeningPosture,
    TopVulnerableService,
};
