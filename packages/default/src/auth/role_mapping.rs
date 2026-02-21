//! Role mapping from OIDC claims to Crystal Forge RBAC roles.
//!
//! Maps OIDC groups/roles claims to local Admin, Operator, and Viewer roles.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Crystal Forge RBAC roles.
///
/// Must match the `auth_role` enum in the database schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Admin,
    Operator,
    Viewer,
}

impl Role {
    /// Convert role to database string representation.
    pub fn as_str(&self) -> &'static str {
        match self {
            Role::Admin => "admin",
            Role::Operator => "operator",
            Role::Viewer => "viewer",
        }
    }

    /// Parse role from database string representation.
    pub fn from_str(s: &str) -> Result<Self, RoleMappingError> {
        match s {
            "admin" => Ok(Role::Admin),
            "operator" => Ok(Role::Operator),
            "viewer" => Ok(Role::Viewer),
            _ => Err(RoleMappingError::InvalidRole(s.to_string())),
        }
    }
}

/// Role mapping configuration.
///
/// Maps OIDC groups/roles to Crystal Forge roles.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoleMappingConfig {
    /// Map OIDC group names to Crystal Forge roles.
    ///
    /// Example:
    /// ```json
    /// {
    ///   "crystal-forge-admins": "admin",
    ///   "crystal-forge-operators": "operator",
    ///   "crystal-forge-viewers": "viewer"
    /// }
    /// ```
    #[serde(default)]
    pub group_role_map: HashMap<String, String>,

    /// Default role to assign when no group mapping matches.
    ///
    /// If None, users without matching groups are denied access (safe-deny).
    #[serde(default)]
    pub default_role: Option<String>,
}

impl Default for RoleMappingConfig {
    /// Default configuration for role mapping.
    ///
    /// **IMPORTANT**: This default is NOT used in production.
    /// Production uses `from_env()` which implements safe-deny (no default role).
    ///
    /// This Default impl exists only for:
    /// - Deserialization fallback when parsing config files
    /// - Test convenience
    fn default() -> Self {
        Self {
            group_role_map: HashMap::new(),
            default_role: None, // Safe-deny: no default role
        }
    }
}

impl RoleMappingConfig {
    /// Load role mapping configuration from environment variables.
    ///
    /// Environment variables:
    /// - CRYSTAL_FORGE_ROLE_MAPPING: JSON object mapping group names to roles
    ///   Example: `{"admins":"admin","operators":"operator","users":"viewer"}`
    /// - CRYSTAL_FORGE_DEFAULT_ROLE: Default role when no group matches (optional)
    ///   Example: `viewer`
    ///
    /// If CRYSTAL_FORGE_DEFAULT_ROLE is not set, uses safe-deny (no default role).
    pub fn from_env() -> Self {
        let group_role_map = match std::env::var("CRYSTAL_FORGE_ROLE_MAPPING") {
            Ok(json_str) => match serde_json::from_str(&json_str) {
                Ok(map) => map,
                Err(e) => {
                    tracing::error!(
                        "Failed to parse CRYSTAL_FORGE_ROLE_MAPPING as JSON: {}. \
                         All OIDC logins will fail unless CRYSTAL_FORGE_DEFAULT_ROLE is set. \
                         Expected format: {{\"group-name\":\"role\"}}",
                        e
                    );
                    HashMap::new()
                }
            },
            Err(_) => {
                tracing::warn!(
                    "CRYSTAL_FORGE_ROLE_MAPPING not set. \
                     All OIDC logins will fail unless CRYSTAL_FORGE_DEFAULT_ROLE is set."
                );
                HashMap::new()
            }
        };

        let default_role = std::env::var("CRYSTAL_FORGE_DEFAULT_ROLE").ok();

        if group_role_map.is_empty() && default_role.is_none() {
            tracing::warn!(
                "Role mapping is completely unconfigured (no mapping and no default role). \
                 All OIDC logins will be denied. Set CRYSTAL_FORGE_ROLE_MAPPING or \
                 CRYSTAL_FORGE_DEFAULT_ROLE to allow access."
            );
        }

        Self {
            group_role_map,
            default_role,
        }
    }

    /// Map OIDC groups to Crystal Forge roles.
    ///
    /// Returns the highest privilege role found, or default role if no matches.
    /// Role precedence: Admin > Operator > Viewer
    ///
    /// # Errors
    ///
    /// Returns `RoleMappingError::NoMatchingRole` if:
    /// - No groups match the mapping
    /// - No default role is configured (safe-deny)
    pub fn map_groups_to_role(&self, groups: &[String]) -> Result<Role, RoleMappingError> {
        let mut matched_roles = Vec::new();

        // Find all matching roles
        for group in groups {
            if let Some(role_str) = self.group_role_map.get(group) {
                let role = Role::from_str(role_str)?;
                matched_roles.push(role);
            }
        }

        // Return highest privilege role
        if matched_roles.contains(&Role::Admin) {
            return Ok(Role::Admin);
        }
        if matched_roles.contains(&Role::Operator) {
            return Ok(Role::Operator);
        }
        if matched_roles.contains(&Role::Viewer) {
            return Ok(Role::Viewer);
        }

        // No matching groups - use default role or deny
        if let Some(default_role_str) = &self.default_role {
            Role::from_str(default_role_str)
        } else {
            Err(RoleMappingError::NoMatchingRole {
                groups: groups.to_vec(),
            })
        }
    }
}

/// Role mapping errors.
#[derive(Debug)]
pub enum RoleMappingError {
    InvalidRole(String),
    NoMatchingRole { groups: Vec<String> },
}

impl std::fmt::Display for RoleMappingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RoleMappingError::InvalidRole(role) => write!(f, "Invalid role: {}", role),
            RoleMappingError::NoMatchingRole { groups } => {
                write!(f, "No matching role for groups: {:?}", groups)
            }
        }
    }
}

impl std::error::Error for RoleMappingError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn role_as_str_matches_database_enum() {
        assert_eq!(Role::Admin.as_str(), "admin");
        assert_eq!(Role::Operator.as_str(), "operator");
        assert_eq!(Role::Viewer.as_str(), "viewer");
    }

    #[test]
    fn role_from_str_parses_valid_roles() {
        assert_eq!(Role::from_str("admin").unwrap(), Role::Admin);
        assert_eq!(Role::from_str("operator").unwrap(), Role::Operator);
        assert_eq!(Role::from_str("viewer").unwrap(), Role::Viewer);
    }

    #[test]
    fn role_from_str_rejects_invalid_roles() {
        assert!(Role::from_str("superuser").is_err());
        assert!(Role::from_str("").is_err());
    }

    #[test]
    fn map_groups_to_role_returns_admin_when_admin_group_present() {
        let mut config = RoleMappingConfig::default();
        config
            .group_role_map
            .insert("admins".to_string(), "admin".to_string());
        config
            .group_role_map
            .insert("users".to_string(), "viewer".to_string());

        let groups = vec!["admins".to_string(), "users".to_string()];
        let role = config.map_groups_to_role(&groups).unwrap();
        assert_eq!(role, Role::Admin);
    }

    #[test]
    fn map_groups_to_role_returns_operator_when_no_admin() {
        let mut config = RoleMappingConfig::default();
        config
            .group_role_map
            .insert("operators".to_string(), "operator".to_string());
        config
            .group_role_map
            .insert("users".to_string(), "viewer".to_string());

        let groups = vec!["operators".to_string(), "users".to_string()];
        let role = config.map_groups_to_role(&groups).unwrap();
        assert_eq!(role, Role::Operator);
    }

    #[test]
    fn map_groups_to_role_returns_viewer_when_only_viewer_group() {
        let mut config = RoleMappingConfig::default();
        config
            .group_role_map
            .insert("users".to_string(), "viewer".to_string());

        let groups = vec!["users".to_string()];
        let role = config.map_groups_to_role(&groups).unwrap();
        assert_eq!(role, Role::Viewer);
    }

    #[test]
    fn map_groups_to_role_returns_default_when_no_match() {
        let mut config = RoleMappingConfig::default();
        config
            .group_role_map
            .insert("admins".to_string(), "admin".to_string());
        config.default_role = Some("viewer".to_string());

        let groups = vec!["unknown-group".to_string()];
        let role = config.map_groups_to_role(&groups).unwrap();
        assert_eq!(role, Role::Viewer);
    }

    #[test]
    fn map_groups_to_role_denies_when_no_match_and_no_default() {
        let mut config = RoleMappingConfig::default();
        config
            .group_role_map
            .insert("admins".to_string(), "admin".to_string());
        config.default_role = None; // Safe-deny

        let groups = vec!["unknown-group".to_string()];
        let result = config.map_groups_to_role(&groups);
        assert!(result.is_err());
        assert!(matches!(
            result,
            Err(RoleMappingError::NoMatchingRole { .. })
        ));
    }

    #[test]
    fn map_groups_to_role_rejects_invalid_role_in_mapping() {
        let mut config = RoleMappingConfig::default();
        config
            .group_role_map
            .insert("superusers".to_string(), "superuser".to_string());

        let groups = vec!["superusers".to_string()];
        let result = config.map_groups_to_role(&groups);
        assert!(result.is_err());
        assert!(matches!(result, Err(RoleMappingError::InvalidRole(_))));
    }

    #[test]
    fn empty_groups_uses_default_role() {
        let mut config = RoleMappingConfig::default();
        config.default_role = Some("viewer".to_string());

        let groups = vec![];
        let role = config.map_groups_to_role(&groups).unwrap();
        assert_eq!(role, Role::Viewer);
    }

    #[test]
    fn empty_groups_denies_when_no_default() {
        let mut config = RoleMappingConfig::default();
        config.default_role = None;

        let groups = vec![];
        let result = config.map_groups_to_role(&groups);
        assert!(result.is_err());
    }
}
