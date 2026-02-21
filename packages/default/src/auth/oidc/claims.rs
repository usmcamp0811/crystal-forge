//! OIDC claim extraction and mapping.

use crate::config::ClaimMappingConfig;
use anyhow::{Context, Result};
use serde_json::Value;
use std::collections::HashMap;

/// Extracted user information from OIDC claims.
#[derive(Debug, Clone)]
pub struct OidcUserInfo {
    /// Unique subject identifier (from `sub` claim)
    pub subject: String,

    /// Email address
    pub email: Option<String>,

    /// Email verified flag
    pub email_verified: bool,

    /// Full display name
    pub display_name: Option<String>,

    /// Given name (first name)
    pub given_name: Option<String>,

    /// Family name (last name)
    pub family_name: Option<String>,

    /// Preferred username
    pub preferred_username: Option<String>,

    /// Roles/groups (extracted from configurable claim)
    pub roles: Vec<String>,

    /// Raw custom claims (for provider-specific attributes)
    pub custom_claims: HashMap<String, Value>,
}

/// OIDC claim extractor with configurable mappings.
#[derive(Debug, Clone)]
pub struct ClaimExtractor {
    config: ClaimMappingConfig,
}

impl ClaimExtractor {
    /// Create a new claim extractor with the given configuration.
    pub fn new(config: ClaimMappingConfig) -> Self {
        Self { config }
    }

    /// Extract user information from OIDC token claims.
    ///
    /// Prioritizes standard typed claims over custom claims to avoid missing
    /// standard fields when providers include them in both locations.
    ///
    /// # Arguments
    ///
    /// * `typed_email` - Email from standard typed claim (if present)
    /// * `typed_email_verified` - Email verified flag from typed claim
    /// * `typed_name` - Full name from typed claim
    /// * `typed_given_name` - Given name from typed claim
    /// * `typed_family_name` - Family name from typed claim
    /// * `typed_preferred_username` - Preferred username from typed claim
    /// * `custom_claims` - Additional custom claims map
    /// * `subject` - The `sub` claim (unique user identifier)
    pub fn extract_user_info(
        &self,
        typed_email: Option<String>,
        typed_email_verified: Option<bool>,
        typed_name: Option<String>,
        typed_given_name: Option<String>,
        typed_family_name: Option<String>,
        typed_preferred_username: Option<String>,
        custom_claims: &HashMap<String, Value>,
        subject: String,
    ) -> Result<OidcUserInfo> {
        // Extract email - prefer typed claim, fall back to custom claim
        let email = typed_email
            .or_else(|| self.extract_string_claim(custom_claims, &self.config.email_claim));

        // Extract email_verified - prefer typed claim, fall back to custom claim
        let email_verified = typed_email_verified
            .or_else(|| {
                custom_claims
                    .get("email_verified")
                    .and_then(|v| v.as_bool())
            })
            .unwrap_or(false);

        // Extract display name - prefer typed claim, fall back to custom claim
        let display_name = typed_name
            .or_else(|| self.extract_string_claim(custom_claims, &self.config.name_claim));

        // Extract given name - prefer typed claim, fall back to custom claim
        let given_name = typed_given_name
            .or_else(|| self.extract_string_claim(custom_claims, &self.config.given_name_claim));

        // Extract family name - prefer typed claim, fall back to custom claim
        let family_name = typed_family_name
            .or_else(|| self.extract_string_claim(custom_claims, &self.config.family_name_claim));

        // Extract preferred username - prefer typed claim, fall back to custom claim
        let preferred_username = typed_preferred_username.or_else(|| {
            self.extract_string_claim(custom_claims, &self.config.preferred_username_claim)
        });

        // Extract roles/groups (only from custom claims)
        let roles = self.extract_roles(custom_claims)?;

        Ok(OidcUserInfo {
            subject,
            email,
            email_verified,
            display_name,
            given_name,
            family_name,
            preferred_username,
            roles,
            custom_claims: custom_claims.clone(),
        })
    }

    /// Extract a string claim from the claims map.
    fn extract_string_claim(
        &self,
        claims: &HashMap<String, Value>,
        claim_name: &str,
    ) -> Option<String> {
        claims
            .get(claim_name)
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    }

    /// Extract roles/groups from claims.
    ///
    /// Supports multiple formats:
    /// - Array of strings: `["admin", "operator"]`
    /// - Single string: `"admin"`
    /// - Comma-separated string: `"admin,operator"`
    fn extract_roles(&self, claims: &HashMap<String, Value>) -> Result<Vec<String>> {
        let roles_claim = claims.get(&self.config.roles_claim);

        let roles = match roles_claim {
            Some(Value::Array(arr)) => {
                // Array of strings
                arr.iter()
                    .filter_map(|v| v.as_str())
                    .map(|s| s.to_string())
                    .collect()
            }
            Some(Value::String(s)) => {
                // Single string or comma-separated
                if s.contains(',') {
                    s.split(',').map(|s| s.trim().to_string()).collect()
                } else {
                    vec![s.clone()]
                }
            }
            _ => {
                // No roles claim or unsupported format
                tracing::debug!(
                    "No roles found in claim '{}' or unsupported format",
                    self.config.roles_claim
                );
                vec![]
            }
        };

        Ok(roles)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn default_config() -> ClaimMappingConfig {
        ClaimMappingConfig::default()
    }

    #[test]
    fn extract_user_info_with_all_claims() {
        let extractor = ClaimExtractor::new(default_config());

        let mut claims = HashMap::new();
        claims.insert("groups".to_string(), json!(["admin", "operator"]));

        let user_info = extractor
            .extract_user_info(
                Some("user@example.com".to_string()),
                Some(true),
                Some("John Doe".to_string()),
                Some("John".to_string()),
                Some("Doe".to_string()),
                Some("johndoe".to_string()),
                &claims,
                "user-123".to_string(),
            )
            .unwrap();

        assert_eq!(user_info.subject, "user-123");
        assert_eq!(user_info.email, Some("user@example.com".to_string()));
        assert!(user_info.email_verified);
        assert_eq!(user_info.display_name, Some("John Doe".to_string()));
        assert_eq!(user_info.given_name, Some("John".to_string()));
        assert_eq!(user_info.family_name, Some("Doe".to_string()));
        assert_eq!(user_info.preferred_username, Some("johndoe".to_string()));
        assert_eq!(user_info.roles, vec!["admin", "operator"]);
    }

    #[test]
    fn extract_roles_from_array() {
        let extractor = ClaimExtractor::new(default_config());

        let mut claims = HashMap::new();
        claims.insert("groups".to_string(), json!(["admin", "operator", "viewer"]));

        let roles = extractor.extract_roles(&claims).unwrap();
        assert_eq!(roles, vec!["admin", "operator", "viewer"]);
    }

    #[test]
    fn extract_roles_from_comma_separated_string() {
        let extractor = ClaimExtractor::new(default_config());

        let mut claims = HashMap::new();
        claims.insert("groups".to_string(), json!("admin,operator,viewer"));

        let roles = extractor.extract_roles(&claims).unwrap();
        assert_eq!(roles, vec!["admin", "operator", "viewer"]);
    }

    #[test]
    fn extract_roles_from_single_string() {
        let extractor = ClaimExtractor::new(default_config());

        let mut claims = HashMap::new();
        claims.insert("groups".to_string(), json!("admin"));

        let roles = extractor.extract_roles(&claims).unwrap();
        assert_eq!(roles, vec!["admin"]);
    }

    #[test]
    fn extract_roles_handles_whitespace_in_comma_separated() {
        let extractor = ClaimExtractor::new(default_config());

        let mut claims = HashMap::new();
        claims.insert("groups".to_string(), json!("admin , operator ,  viewer  "));

        let roles = extractor.extract_roles(&claims).unwrap();
        assert_eq!(roles, vec!["admin", "operator", "viewer"]);
    }

    #[test]
    fn extract_roles_missing_claim() {
        let extractor = ClaimExtractor::new(default_config());
        let claims = HashMap::new();

        let roles = extractor.extract_roles(&claims).unwrap();
        assert!(roles.is_empty());
    }

    #[test]
    fn extract_user_info_with_minimal_claims() {
        let extractor = ClaimExtractor::new(default_config());

        let claims = HashMap::new();

        let user_info = extractor
            .extract_user_info(
                None,
                None,
                None,
                None,
                None,
                None,
                &claims,
                "user-456".to_string(),
            )
            .unwrap();

        assert_eq!(user_info.subject, "user-456");
        assert_eq!(user_info.email, None);
        assert!(!user_info.email_verified);
        assert_eq!(user_info.display_name, None);
        assert!(user_info.roles.is_empty());
    }
}
