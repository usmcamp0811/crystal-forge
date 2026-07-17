//! Environment-related UI components.
//!
//! Components for displaying and managing environments, including
//! cards, forms, and modals.

use dioxus::prelude::*;
use uuid::Uuid;

/// Policy option for environment requirements.
#[derive(Clone, Debug, PartialEq)]
pub struct PolicyOption {
    pub id: Uuid,
    pub name: String,
    pub description: String,
}

/// Environment item for display.
#[derive(Clone, Debug, PartialEq)]
pub struct EnvironmentItem {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub color_hex: String,
    pub system_count: usize,
    pub required_policy_ids: Vec<Uuid>,
    /// Real backend data from `EnvironmentSummary.rollup`.
    pub health: EnvironmentHealthBreakdown,
    /// Real backend data from `EnvironmentSummary.rollup`.
    pub cve_critical_high: usize,
    /// Real backend data from `EnvironmentSummary.rollup`.
    pub flake_names: Vec<String>,
    /// Real backend data persisted on `environments.default_policy` (TASK-392).
    pub default_policy: Option<EnvironmentDeploymentPolicy>,
    /// Real backend data derived from the environment's assigned cache
    /// destination. `status` reflects `cache_destinations.enabled`, not a
    /// live reachability probe.
    pub cache: Option<EnvironmentCacheSummary>,
    /// Real backend data persisted on `environments.auto_sync` (TASK-392).
    pub auto_sync: Option<bool>,
    /// Real backend data persisted on `environments.requires_approval` (TASK-392).
    pub requires_approval: Option<bool>,
    /// Real backend data persisted on `environments.is_production` (TASK-392).
    pub is_production: Option<bool>,
    /// Real backend data: count of `user_environment_memberships` rows for
    /// this environment.
    pub role_assignment_count: Option<usize>,
}

/// Health-state counts for active systems in an environment.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EnvironmentHealthBreakdown {
    pub active: usize,
    pub healthy: usize,
    pub warning: usize,
    pub critical: usize,
    pub offline: usize,
}

impl EnvironmentHealthBreakdown {
    pub fn total(&self) -> usize {
        self.healthy + self.warning + self.critical + self.offline
    }
}

/// Display value for the default deployment policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EnvironmentDeploymentPolicy {
    Manual,
    AutoLatest,
    Pinned,
}

impl EnvironmentDeploymentPolicy {
    pub fn id(self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::AutoLatest => "auto_latest",
            Self::Pinned => "pinned",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::AutoLatest => "auto_latest",
            Self::Pinned => "pinned",
        }
    }
}

/// Display-only cache summary placeholder until TASK-360 lands.
#[derive(Clone, Debug, PartialEq)]
pub struct EnvironmentCacheSummary {
    pub name: String,
    pub url: String,
    pub cache_type: String,
    pub status: String,
}

/// Draft for creating a new environment.
#[derive(Clone, Debug, PartialEq)]
pub struct NewEnvironmentDraft {
    pub name: String,
    pub description: String,
    pub color_hex: String,
    pub required_policy_ids: Vec<Uuid>,
}

/// Draft for editing an environment.
#[derive(Clone, Debug, PartialEq)]
pub struct EditEnvironmentDraft {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub color_hex: String,
}

/// Unified Add/Edit modal draft for the CrystalForgelatest Environments surface.
#[derive(Clone, Debug, PartialEq)]
pub struct EnvironmentFormDraft {
    pub id: Option<Uuid>,
    pub name: String,
    pub description: String,
    pub color_hex: String,
    pub required_policy_ids: Vec<Uuid>,
    pub default_policy: Option<EnvironmentDeploymentPolicy>,
    pub auto_sync: Option<bool>,
    pub requires_approval: Option<bool>,
    pub is_production: Option<bool>,
}

mod add_environment_form;
mod environment_card;
mod environment_form_modal;
mod remove_environment_dialog;

pub use environment_card::{EnvironmentCard, EnvironmentTable};
pub use environment_form_modal::{EnvironmentFormModal, validate_environment_form};
pub use remove_environment_dialog::RemoveEnvironmentDialog;

// Re-export helper functions
pub use add_environment_form::validate_environment;

/// Get the default required policy ID (Crystal Forge Agent).
pub fn required_agent_policy_id(policy_library: &[PolicyOption]) -> Uuid {
    policy_library
        .iter()
        .find(|policy| policy.name == "Require Crystal Forge Agent")
        .map(|policy| policy.id)
        .unwrap_or_else(|| Uuid::from_u128(1))
}

/// Get policy names for a list of policy IDs.
pub fn required_policy_names(ids: &[Uuid], policy_library: &[PolicyOption]) -> Vec<String> {
    ids.iter()
        .filter_map(|id| {
            policy_library
                .iter()
                .find(|policy| policy.id == *id)
                .map(|policy| policy.name.clone())
        })
        .collect()
}

/// Get environment name for an ID.
pub fn environment_name_for_id(id: Uuid, environments: &[EnvironmentItem]) -> String {
    environments
        .iter()
        .find(|env| env.id == id)
        .map(|env| env.name.clone())
        .unwrap_or_else(|| "Environment".to_string())
}

/// Normalize an optional string.
pub fn normalize_optional(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Normalize a hex color value.
pub fn normalize_color_hex(value: &str) -> String {
    let trimmed = value.trim();
    if looks_like_hex_color(trimmed) {
        trimmed.to_uppercase()
    } else {
        "#4F46E5".to_string()
    }
}

/// Check if a string looks like a hex color.
pub fn looks_like_hex_color(value: &str) -> bool {
    if value.len() != 7 || !value.starts_with('#') {
        return false;
    }
    value[1..].chars().all(|ch| ch.is_ascii_hexdigit())
}

/// Convert hex color to rgba with alpha.
pub fn with_alpha(hex: &str, alpha: f32) -> String {
    let color = normalize_color_hex(hex);
    let r = u8::from_str_radix(&color[1..3], 16).unwrap_or(79);
    let g = u8::from_str_radix(&color[3..5], 16).unwrap_or(70);
    let b = u8::from_str_radix(&color[5..7], 16).unwrap_or(229);
    format!("rgba({r}, {g}, {b}, {alpha})")
}

/// Get the policy library.
pub fn policy_library() -> Vec<PolicyOption> {
    vec![
        PolicyOption {
            id: Uuid::from_u128(1),
            name: "Require Crystal Forge Agent".to_string(),
            description: "Ensure Crystal Forge services are enabled on the target.".to_string(),
        },
        PolicyOption {
            id: Uuid::from_u128(2),
            name: "Require Packages".to_string(),
            description: "Guarantee required package set is installed.".to_string(),
        },
        PolicyOption {
            id: Uuid::from_u128(3),
            name: "Custom Check".to_string(),
            description: "Evaluate environment-specific Nix policy expression.".to_string(),
        },
    ]
}
