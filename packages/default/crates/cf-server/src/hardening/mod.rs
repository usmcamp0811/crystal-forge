//! Systemd hardening analysis for NixOS configurations.
//!
//! This module provides static analysis of systemd service configurations
//! extracted from NixOS flake outputs, scoring services based on security
//! hardening directives.

pub mod scanner;
pub mod scoring;
pub mod types;

pub use scanner::HardeningScanner;
pub use scoring::{HARDENING_DIRECTIVES, HardeningDirective, calculate_service_score};
pub use types::{
    DirectiveDetail, HardeningJustification, HardeningScan, RiskLevel, ScanStatus,
    ServiceHardeningResult,
};
