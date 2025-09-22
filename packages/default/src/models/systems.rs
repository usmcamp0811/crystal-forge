use crate::models::public_key::PublicKey;
use crate::queries::systems::insert_system;
use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use sqlx::PgPool;
use std::option::Option;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DeploymentPolicy {
    #[serde(rename = "manual")]
    Manual,
    #[serde(rename = "auto_latest")]
    AutoLatest,
    #[serde(rename = "pinned")]
    Pinned,
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

#[derive(Debug, FromRow, Serialize, Deserialize)]
pub struct System {
    pub id: Uuid,
    pub hostname: String,
    pub environment_id: Option<Uuid>,
    pub is_active: bool,
    pub public_key: PublicKey,
    pub flake_id: Option<i32>,
    pub derivation: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    // pub desired_derivation: Option<String>,
    // pub deployment_policy: String, // Will be converted to/from DeploymentPolicy enum
    // pub server_public_key: Option<String>,
}

impl System {
    /// Creates a new validated System instance and inserts it using `insert_system()`.
    /// UUID & derivation are left for the database or later processing.
    pub async fn new(
        pool: &PgPool,
        hostname: String,
        environment_id: Option<Uuid>,
        is_active: bool,
        public_key_base64: String,
        flake_id: Option<i32>,
    ) -> Result<System> {
        let public_key = PublicKey::from_base64(&public_key_base64, &hostname)?;
        let system = System {
            id: Uuid::nil(), // placeholder; DB will assign real UUID
            hostname,
            environment_id,
            is_active,
            public_key,
            flake_id,
            derivation: "".into(), // leave empty; DB or later logic sets it
            created_at: chrono::Utc::now(), // placeholder; overwritten by DB
            updated_at: chrono::Utc::now(), // placeholder; overwritten by DB
                                   // desired_derivation: None,
                                   // deployment_policy: DeploymentPolicy::Manual.to_string(),
                                   // server_public_key: None,
        };
        insert_system(pool, &system).await
    }

    // /// Get the deployment policy as an enum
    // pub fn get_deployment_policy(&self) -> Result<DeploymentPolicy> {
    //     self.deployment_policy.parse()
    // }
    //
    // /// Set the deployment policy from an enum
    // pub fn set_deployment_policy(&mut self, policy: DeploymentPolicy) {
    //     self.deployment_policy = policy.to_string();
    // }
    //
    // /// Check if the system has a desired derivation set
    // pub fn has_pending_deployment(&self) -> bool {
    //     self.desired_derivation.is_some()
    // }
    //
    // /// Check if the system is configured for automatic deployments
    // pub fn is_auto_deployment_enabled(&self) -> bool {
    //     matches!(
    //         self.get_deployment_policy(),
    //         Ok(DeploymentPolicy::AutoLatest)
    //     )
    // }
}
