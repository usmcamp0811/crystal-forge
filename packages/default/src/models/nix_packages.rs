use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, FromRow, Serialize, Deserialize)]
pub struct NixPackage {
    pub derivation_path: String, // Primary key
    pub name: String,            // Full package name for display
    pub pname: String,           // Package name for CVE matching
    pub version: String,         // Version for vulnerability assessment
    pub created_at: DateTime<Utc>,
}

impl NixPackage {
    /// Create a new NixPackage instance
    pub fn new(derivation_path: String, name: String, pname: String, version: String) -> Self {
        Self {
            derivation_path,
            name,
            pname,
            version,
            created_at: Utc::now(),
        }
    }

    /// Extract version components for comparison
    pub fn version_components(&self) -> Vec<u32> {
        self.version
            .split('.')
            .filter_map(|part| part.parse().ok())
            .collect()
    }

    /// Check if this package version is likely affected by a CVE with a fixed version
    pub fn is_likely_affected(&self, fixed_version: &str) -> bool {
        // Simple version comparison - in production you'd want more sophisticated logic
        self.version < *fixed_version
    }
}

impl std::fmt::Display for NixPackage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}@{}", self.pname, self.version)
    }
}
