use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, FromRow, Serialize, Deserialize)]
pub struct PackageVulnerability {
    pub id: Uuid,
    pub derivation_path: String,
    pub cve_id: String,
    pub is_whitelisted: bool,
    pub whitelist_reason: Option<String>,
    pub whitelist_expires_at: Option<DateTime<Utc>>,
    pub fixed_version: Option<String>,
    pub detection_method: String, // vulnix, trivy, osv, etc.
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl PackageVulnerability {
    /// Create a new vulnerability record
    pub fn new(derivation_path: String, cve_id: String, detection_method: String) -> Self {
        Self {
            id: Uuid::new_v4(),
            derivation_path,
            cve_id,
            is_whitelisted: false,
            whitelist_reason: None,
            whitelist_expires_at: None,
            fixed_version: None,
            detection_method,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    /// Whitelist this vulnerability with a reason
    pub fn whitelist(&mut self, reason: String, expires_at: Option<DateTime<Utc>>) {
        self.is_whitelisted = true;
        self.whitelist_reason = Some(reason);
        self.whitelist_expires_at = expires_at;
        self.updated_at = Utc::now();
    }

    /// Remove whitelist status
    pub fn remove_whitelist(&mut self) {
        self.is_whitelisted = false;
        self.whitelist_reason = None;
        self.whitelist_expires_at = None;
        self.updated_at = Utc::now();
    }

    /// Check if this vulnerability is currently whitelisted
    pub fn is_currently_whitelisted(&self) -> bool {
        if !self.is_whitelisted {
            return false;
        }

        // Check if whitelist has expired
        match self.whitelist_expires_at {
            Some(expires) => expires > Utc::now(),
            None => true, // No expiration means permanent whitelist
        }
    }

    /// Check if vulnerability affects a specific package version
    pub fn affects_version(&self, version: &str) -> bool {
        match &self.fixed_version {
            Some(fixed) => *version < **fixed,
            None => true, // No fixed version means all versions affected
        }
    }
}
