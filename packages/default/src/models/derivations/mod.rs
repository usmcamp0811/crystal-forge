use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use tracing::error;

// Core model definitions
pub mod build;
pub mod cache;
pub mod eval;
pub mod utils;

// Re-export everything for backward compatibility
pub use build::*;
pub use cache::*;
pub use eval::*;
pub use utils::*;

#[derive(Debug, FromRow, Serialize, Deserialize)]
pub struct Derivation {
    pub id: i32,
    pub commit_id: Option<i32>,          // Changed from i32 to Option<i32>
    pub derivation_type: DerivationType, // "nixos" or "package"
    pub derivation_name: String, // display name (hostname for nixos, package name for packages)
    pub derivation_path: Option<String>, // populated post-build
    pub scheduled_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub started_at: Option<DateTime<Utc>>,
    pub attempt_count: i32,
    pub evaluation_duration_ms: Option<i32>,
    pub error_message: Option<String>,
    // pub parent_derivation_id: Option<i32>, // for hierarchical relationships (packages → systems)
    pub pname: Option<String>,   // Nix package name (for packages)
    pub version: Option<String>, // package version (for packages)
    pub status_id: i32,          // foreign key to derivation_statuses table
    pub derivation_target: Option<String>,
    #[serde(default)]
    pub build_elapsed_seconds: Option<i32>,
    #[serde(default)]
    pub build_current_target: Option<String>,
    #[serde(default)]
    pub build_last_activity_seconds: Option<i32>,
    #[serde(default)]
    pub build_last_heartbeat: Option<DateTime<Utc>>,
    #[serde(default)]
    pub cf_agent_enabled: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type, PartialEq)]
#[sqlx(type_name = "text")]
#[sqlx(rename_all = "lowercase")]
pub enum DerivationType {
    #[sqlx(rename = "nixos")]
    NixOS,
    #[sqlx(rename = "package")]
    Package,
}

// Status information from the derivation_statuses table
#[derive(Debug, FromRow, Serialize, Deserialize)]
pub struct DerivationStatus {
    pub id: i32,
    pub name: String,
    pub description: Option<String>,
    pub is_terminal: bool,
    pub is_success: bool,
    pub display_order: i32,
}

// SQLx requires this for deserialization - make it safe
impl From<String> for DerivationType {
    fn from(s: String) -> Self {
        match s.as_str() {
            "nixos" => DerivationType::NixOS,
            "package" => DerivationType::Package,
            _ => {
                // Log the error but provide a default instead of panicking
                error!(
                    "Warning: Unknown DerivationType '{}', defaulting to NixOS",
                    s
                );
                DerivationType::NixOS
            }
        }
    }
}

impl ToString for DerivationType {
    fn to_string(&self) -> String {
        match self {
            DerivationType::NixOS => "nixos".into(),
            DerivationType::Package => "package".into(),
        }
    }
}

impl Derivation {
    /// Check if this derivation has Crystal Forge agent enabled
    pub fn is_cf_agent_enabled(&self) -> bool {
        self.cf_agent_enabled.unwrap_or(false)
    }

    /// Set the Crystal Forge agent enabled status
    pub fn set_cf_agent_enabled(&mut self, enabled: bool) {
        self.cf_agent_enabled = Some(enabled);
    }

    /// Check if this derivation is safe for deployment
    /// (has Crystal Forge agent enabled and is a successful build)
    pub fn is_deployment_safe(&self) -> bool {
        self.is_cf_agent_enabled()
    }

    /// Check if this derivation is eligible for deployment
    /// Must be a NixOS derivation with CF agent enabled
    pub fn is_deployable(&self) -> bool {
        matches!(self.derivation_type, DerivationType::NixOS) && self.is_cf_agent_enabled()
    }

    pub async fn summary(&self) -> anyhow::Result<String> {
        let pool = crate::models::config::CrystalForgeConfig::db_pool().await?;

        if let Some(commit_id) = self.commit_id {
            let commit = crate::queries::commits::get_commit_by_id(&pool, commit_id).await?;
            let flake = crate::queries::flakes::get_flake_by_id(&pool, commit.flake_id).await?;
            Ok(format!(
                "{}@{} ({})",
                flake.name, commit.git_commit_hash, self.derivation_name
            ))
        } else {
            // For packages without a commit (discovered during CVE scans)
            Ok(format!(
                "package {} ({})",
                self.derivation_name,
                self.pname.as_deref().unwrap_or("unknown")
            ))
        }
    }
}

// For backward compatibility during migration
pub type EvaluationTarget = Derivation;
pub type TargetType = DerivationType;
