use crate::models::public_key::PublicKey;
use crate::queries::systems::insert_system;
use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use sqlx::PgPool;
use std::option::Option;
use uuid::Uuid;

/// Identifies deployment authority independently of configuration assurance.
///
/// All modes permit evaluation, builds and scans under their applicable
/// policies. Only [`Self::Managed`] permits deployment authorization. The mode
/// does not establish runtime health, evidence freshness or registration activity.
/// Existing clients that omit the mode retain managed-system semantics.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "text", rename_all = "snake_case")]
pub enum ManagementMode {
    /// Represents a system whose enrolled agent can receive authorized deploys.
    #[default]
    Managed,
    /// Represents a system deployed outside Crystal Forge's authority.
    ///
    /// Configuration evidence does not establish what this system is running.
    External,
    /// Represents a configuration for assurance, not a running machine.
    Reference,
}

impl ManagementMode {
    /// Returns the stable database and API representation of this mode.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Managed => "managed",
            Self::External => "external",
            Self::Reference => "reference",
        }
    }

    /// Returns whether this mode permits deployment authorization.
    ///
    /// A positive result is necessary, not sufficient: enrollment, registration
    /// activity, user authorization and deployment policies must also permit it.
    pub const fn permits_deployment(self) -> bool {
        matches!(self, Self::Managed)
    }
}

impl std::fmt::Display for ManagementMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for ManagementMode {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self> {
        match value {
            "managed" => Ok(Self::Managed),
            "external" => Ok(Self::External),
            "reference" => Ok(Self::Reference),
            _ => anyhow::bail!("Invalid management mode"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DeploymentPolicy {
    #[serde(rename = "manual")]
    Manual,
    #[serde(rename = "auto_latest")]
    AutoLatest,
    #[serde(rename = "pinned")]
    Pinned,
}

#[cfg(test)]
mod management_mode_tests {
    use super::ManagementMode;

    #[test]
    fn management_modes_use_exact_database_and_api_values() {
        for (mode, value) in [
            (ManagementMode::Managed, "managed"),
            (ManagementMode::External, "external"),
            (ManagementMode::Reference, "reference"),
        ] {
            assert_eq!(mode.as_str(), value);
            assert_eq!(mode.to_string(), value);
            assert_eq!(value.parse::<ManagementMode>().unwrap(), mode);
            let json = serde_json::to_string(&mode).unwrap();
            assert_eq!(json, format!("\"{value}\""));
            assert_eq!(serde_json::from_str::<ManagementMode>(&json).unwrap(), mode);
        }
    }

    #[test]
    fn invalid_modes_do_not_acquire_managed_authority() {
        for value in ["", "Managed", "EXTERNAL", "external ", "unknown", "manual"] {
            assert!(value.parse::<ManagementMode>().is_err());
            assert!(serde_json::from_value::<ManagementMode>(serde_json::json!(value)).is_err());
        }
    }

    #[test]
    fn only_managed_mode_permits_deployment_authorization() {
        assert_eq!(ManagementMode::default(), ManagementMode::Managed);
        assert!(ManagementMode::Managed.permits_deployment());
        assert!(!ManagementMode::External.permits_deployment());
        assert!(!ManagementMode::Reference.permits_deployment());
    }
}

impl std::fmt::Display for DeploymentPolicy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DeploymentPolicy::Manual => write!(f, "manual"),
            DeploymentPolicy::AutoLatest => write!(f, "auto_latest"),
            DeploymentPolicy::Pinned => write!(f, "pinned"),
        }
    }
}

impl std::str::FromStr for DeploymentPolicy {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "manual" => Ok(DeploymentPolicy::Manual),
            "auto_latest" => Ok(DeploymentPolicy::AutoLatest),
            "pinned" => Ok(DeploymentPolicy::Pinned),
            _ => Err(anyhow::anyhow!("Invalid deployment policy: {}", s)),
        }
    }
}

/// Represents a registered configuration and its independent agent authority.
///
/// Storage requires a key only for managed resources. Parent associations apply
/// only to external resources and do not establish runtime evidence.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct System {
    pub id: Uuid,
    pub hostname: String,
    pub environment_id: Option<Uuid>,
    pub is_active: bool,
    /// Identifies deployment authority; omitted legacy input defaults to managed.
    #[serde(default)]
    pub management_mode: ManagementMode,
    /// Records the server-owned re-enrollment boundary; None preserves legacy semantics.
    /// Authentication retains this value to reject requests from an earlier enrollment.
    #[serde(default)]
    #[sqlx(default)]
    pub agent_enrolled_at: Option<DateTime<Utc>>,
    /// Contains the enrolled agent key only for managed resources.
    pub public_key: Option<PublicKey>,
    /// Associates an external resource with a host, without granting report authority.
    pub parent_host_id: Option<Uuid>,
    pub flake_id: Option<i32>,
    pub derivation: String,
    pub system_configuration_name: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub desired_target: Option<String>,
    pub deployment_policy: String, // Will be converted to/from DeploymentPolicy enum
}

impl System {
    /// Creates a managed system with a validated agent key using `insert_system()`.
    /// UUID & derivation are left for the database or later processing.
    ///
    /// # Errors
    /// Returns an error for an invalid key or policy, a conflicting nonmanaged
    /// registration, or a database failure. This constructor never converts modes.
    pub async fn new(
        pool: &PgPool,
        hostname: String,
        environment_id: Option<Uuid>,
        is_active: bool,
        public_key_base64: String, // Fixed parameter name
        flake_id: Option<i32>,
        system_configuration_name: Option<String>,
        desired_target: Option<String>,
        deployment_policy: String,
    ) -> Result<Self> {
        let public_key = PublicKey::from_base64(&public_key_base64, &hostname)?;
        let _deployment_policy_enum = deployment_policy.parse::<DeploymentPolicy>()?; // Validate it parses correctly

        let system = System {
            id: Uuid::nil(), // placeholder; DB will assign real UUID
            hostname,
            environment_id,
            is_active,
            management_mode: ManagementMode::Managed,
            agent_enrolled_at: None,
            public_key: Some(public_key),
            parent_host_id: None,
            flake_id,
            derivation: "".into(), // leave empty; DB or later logic sets it
            system_configuration_name,
            created_at: chrono::Utc::now(), // placeholder; overwritten by DB
            updated_at: chrono::Utc::now(), // placeholder; overwritten by DB
            desired_target,
            deployment_policy, // Use the input parameter (as string for DB storage)
        };
        insert_system(pool, &system).await
    }

    /// Get the deployment policy as an enum
    pub fn get_deployment_policy(&self) -> Result<DeploymentPolicy> {
        self.deployment_policy.parse()
    }

    /// Set the deployment policy from an enum
    pub fn set_deployment_policy(&mut self, policy: DeploymentPolicy) {
        self.deployment_policy = policy.to_string();
    }

    /// Check if the system has a desired derivation set
    pub fn has_pending_deployment(&self) -> bool {
        self.desired_target.is_some()
    }

    pub fn configuration_name(&self) -> &str {
        self.system_configuration_name
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or(&self.hostname)
    }

    /// Check if the system is configured for automatic deployments
    pub fn is_auto_deployment_enabled(&self) -> bool {
        matches!(
            self.get_deployment_policy(),
            Ok(DeploymentPolicy::AutoLatest)
        )
    }
}
