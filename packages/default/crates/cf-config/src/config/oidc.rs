//! OIDC provider configuration.

use serde::{Deserialize, Serialize};
use std::num::NonZeroU32;

/// OIDC provider configuration.
///
/// Supports standard OIDC providers (Authentik, Keycloak, Entra, Okta, generic).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OidcConfig {
    /// OIDC issuer URL (e.g., https://auth.example.com/realms/myrealm)
    pub issuer_url: String,

    /// OAuth2 client ID
    pub client_id: String,

    /// OAuth2 client secret
    pub client_secret: String,

    /// Redirect URI for authorization code flow
    /// (e.g., https://crystalforge.example.com/api/auth/callback)
    pub redirect_uri: String,

    /// OAuth2 scopes to request (default: openid profile email)
    #[serde(default = "default_scopes")]
    pub scopes: Vec<String>,

    /// Claim mapping configuration
    #[serde(default)]
    pub claims: ClaimMappingConfig,

    /// Maximum successful OIDC entitlement sync age in seconds (default: 86400).
    ///
    /// The range is 1..=u32::MAX, exactly representable as SQL f64 seconds.
    /// This policy bounds OIDC-owned PAT eligibility; browser sessions retain
    /// their existing expiry policy. Zero cannot disable the freshness check.
    #[serde(default = "default_entitlement_max_age_seconds")]
    pub entitlement_max_age_seconds: NonZeroU32,
}

/// Configuration for extracting claims from OIDC tokens.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaimMappingConfig {
    /// Claim containing user's email (default: "email")
    #[serde(default = "default_email_claim")]
    pub email_claim: String,

    /// Claim containing user's display name (default: "name")
    #[serde(default = "default_name_claim")]
    pub name_claim: String,

    /// Claim containing user's given name (default: "given_name")
    #[serde(default = "default_given_name_claim")]
    pub given_name_claim: String,

    /// Claim containing user's family name (default: "family_name")
    #[serde(default = "default_family_name_claim")]
    pub family_name_claim: String,

    /// Claim containing role/group information (default: "groups")
    /// This is provider-specific and may need customization.
    #[serde(default = "default_roles_claim")]
    pub roles_claim: String,

    /// Optional: claim containing preferred username (default: "preferred_username")
    #[serde(default = "default_preferred_username_claim")]
    pub preferred_username_claim: String,
}

impl Default for ClaimMappingConfig {
    fn default() -> Self {
        Self {
            email_claim: default_email_claim(),
            name_claim: default_name_claim(),
            given_name_claim: default_given_name_claim(),
            family_name_claim: default_family_name_claim(),
            roles_claim: default_roles_claim(),
            preferred_username_claim: default_preferred_username_claim(),
        }
    }
}

fn default_scopes() -> Vec<String> {
    vec![
        "openid".to_string(),
        "profile".to_string(),
        "email".to_string(),
    ]
}

fn default_entitlement_max_age_seconds() -> NonZeroU32 {
    const DEFAULT: NonZeroU32 = NonZeroU32::new(86_400).unwrap();
    DEFAULT
}

fn parse_entitlement_max_age_seconds(value: Option<&str>) -> anyhow::Result<NonZeroU32> {
    value.map_or_else(
        || Ok(default_entitlement_max_age_seconds()),
        |value| {
            value.parse::<NonZeroU32>().map_err(|_| {
                anyhow::anyhow!(
                    "CRYSTAL_FORGE_OIDC_ENTITLEMENT_MAX_AGE_SECONDS must be an integer in 1..=4294967295"
                )
            })
        },
    )
}

fn default_email_claim() -> String {
    "email".to_string()
}

fn default_name_claim() -> String {
    "name".to_string()
}

fn default_given_name_claim() -> String {
    "given_name".to_string()
}

fn default_family_name_claim() -> String {
    "family_name".to_string()
}

fn default_roles_claim() -> String {
    "groups".to_string()
}

fn default_preferred_username_claim() -> String {
    "preferred_username".to_string()
}

impl OidcConfig {
    /// Returns the positive entitlement freshness bound in seconds for queries.
    pub fn entitlement_max_age_seconds(&self) -> u64 {
        u64::from(self.entitlement_max_age_seconds.get())
    }

    /// Determine if the application is running in a secure (HTTPS) context.
    ///
    /// This is used to decide whether to use `__Host-` prefixed cookies
    /// and the `Secure` flag. In development with HTTP, we use regular cookies.
    ///
    /// Returns true if redirect_uri starts with "https://".
    pub fn is_secure_context(&self) -> bool {
        self.redirect_uri.starts_with("https://")
    }

    /// Get bootstrap admin group mapping if configured.
    ///
    /// This allows initial admin access setup via environment variable.
    /// Set CRYSTAL_FORGE_OIDC_BOOTSTRAP_ADMIN_GROUP to the OIDC group name
    /// that should be granted admin access on first startup.
    ///
    /// Example:
    /// ```bash
    /// CRYSTAL_FORGE_OIDC_BOOTSTRAP_ADMIN_GROUP=admin
    /// CRYSTAL_FORGE_OIDC_BOOTSTRAP_ADMIN_GROUP=platform-admins
    /// ```
    ///
    /// This is idempotent - if the mapping already exists, it won't be recreated.
    /// This allows you to bootstrap the first admin user, who can then configure
    /// additional mappings via the admin UI.
    pub fn bootstrap_admin_group() -> Option<String> {
        std::env::var("CRYSTAL_FORGE_OIDC_BOOTSTRAP_ADMIN_GROUP").ok()
    }

    /// Load OIDC configuration from environment variables.
    ///
    /// Required environment variables:
    /// - CRYSTAL_FORGE_OIDC_ISSUER_URL
    /// - CRYSTAL_FORGE_OIDC_CLIENT_ID
    /// - CRYSTAL_FORGE_OIDC_CLIENT_SECRET
    /// - CRYSTAL_FORGE_OIDC_REDIRECT_URI
    ///
    /// Optional:
    /// - CRYSTAL_FORGE_OIDC_SCOPES (comma-separated, defaults to "openid,profile,email")
    /// - CRYSTAL_FORGE_OIDC_EMAIL_CLAIM (default: "email")
    /// - CRYSTAL_FORGE_OIDC_NAME_CLAIM (default: "name")
    /// - CRYSTAL_FORGE_OIDC_ROLES_CLAIM (default: "groups")
    /// - CRYSTAL_FORGE_OIDC_BOOTSTRAP_ADMIN_GROUP (optional initial admin group mapping)
    /// - CRYSTAL_FORGE_OIDC_ENTITLEMENT_MAX_AGE_SECONDS (default: 86400)
    ///
    /// # Errors
    /// Returns an error for missing required variables or an entitlement maximum
    /// age outside 1..=4294967295 seconds, malformed text or non-Unicode input.
    pub fn from_env() -> anyhow::Result<Self> {
        let max_age = match std::env::var("CRYSTAL_FORGE_OIDC_ENTITLEMENT_MAX_AGE_SECONDS") {
            Ok(value) => Some(value),
            Err(std::env::VarError::NotPresent) => None,
            Err(std::env::VarError::NotUnicode(_)) => {
                anyhow::bail!("CRYSTAL_FORGE_OIDC_ENTITLEMENT_MAX_AGE_SECONDS must be Unicode")
            }
        };
        let entitlement_max_age_seconds = parse_entitlement_max_age_seconds(max_age.as_deref())?;
        let issuer_url = std::env::var("CRYSTAL_FORGE_OIDC_ISSUER_URL")
            .map_err(|_| anyhow::anyhow!("CRYSTAL_FORGE_OIDC_ISSUER_URL not set"))?;
        let client_id = std::env::var("CRYSTAL_FORGE_OIDC_CLIENT_ID")
            .map_err(|_| anyhow::anyhow!("CRYSTAL_FORGE_OIDC_CLIENT_ID not set"))?;
        let client_secret = std::env::var("CRYSTAL_FORGE_OIDC_CLIENT_SECRET")
            .map_err(|_| anyhow::anyhow!("CRYSTAL_FORGE_OIDC_CLIENT_SECRET not set"))?;
        let redirect_uri = std::env::var("CRYSTAL_FORGE_OIDC_REDIRECT_URI")
            .map_err(|_| anyhow::anyhow!("CRYSTAL_FORGE_OIDC_REDIRECT_URI not set"))?;

        let scopes = std::env::var("CRYSTAL_FORGE_OIDC_SCOPES")
            .ok()
            .map(|s| s.split(',').map(|s| s.trim().to_string()).collect())
            .unwrap_or_else(default_scopes);

        let claims = ClaimMappingConfig {
            email_claim: std::env::var("CRYSTAL_FORGE_OIDC_EMAIL_CLAIM")
                .unwrap_or_else(|_| default_email_claim()),
            name_claim: std::env::var("CRYSTAL_FORGE_OIDC_NAME_CLAIM")
                .unwrap_or_else(|_| default_name_claim()),
            given_name_claim: std::env::var("CRYSTAL_FORGE_OIDC_GIVEN_NAME_CLAIM")
                .unwrap_or_else(|_| default_given_name_claim()),
            family_name_claim: std::env::var("CRYSTAL_FORGE_OIDC_FAMILY_NAME_CLAIM")
                .unwrap_or_else(|_| default_family_name_claim()),
            roles_claim: std::env::var("CRYSTAL_FORGE_OIDC_ROLES_CLAIM")
                .unwrap_or_else(|_| default_roles_claim()),
            preferred_username_claim: std::env::var("CRYSTAL_FORGE_OIDC_PREFERRED_USERNAME_CLAIM")
                .unwrap_or_else(|_| default_preferred_username_claim()),
        };

        Ok(Self {
            issuer_url,
            client_id,
            client_secret,
            redirect_uri,
            scopes,
            claims,
            entitlement_max_age_seconds,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entitlement_policy_defaults_and_validates_environment_range() {
        assert_eq!(
            parse_entitlement_max_age_seconds(None).unwrap().get(),
            86_400
        );
        for value in ["1", "86400", "4294967295"] {
            assert_eq!(
                parse_entitlement_max_age_seconds(Some(value))
                    .unwrap()
                    .get(),
                value.parse::<u32>().unwrap()
            );
        }
        for value in ["0", "-1", "4294967296", "1.5", "NaN", "", " 1", "invalid"] {
            assert!(parse_entitlement_max_age_seconds(Some(value)).is_err());
        }
    }

    #[test]
    fn entitlement_policy_deserialization_rejects_zero_and_malformed_values() {
        let base = serde_json::json!({
            "issuer_url": "https://auth.example.com",
            "client_id": "test",
            "client_secret": "secret",
            "redirect_uri": "https://app.example.com/callback"
        });
        let default: OidcConfig = serde_json::from_value(base.clone()).unwrap();
        assert_eq!(default.entitlement_max_age_seconds(), 86_400);
        for value in [serde_json::json!(1), serde_json::json!(u32::MAX)] {
            let mut config = base.clone();
            config["entitlement_max_age_seconds"] = value.clone();
            let policy: OidcConfig = serde_json::from_value(config).unwrap();
            assert_eq!(
                policy.entitlement_max_age_seconds(),
                value.as_u64().unwrap()
            );
        }
        for value in [
            serde_json::json!(0),
            serde_json::json!(-1),
            serde_json::json!(4_294_967_296_u64),
            serde_json::json!(1.5),
            serde_json::json!("86400"),
            serde_json::json!(null),
            serde_json::json!(true),
        ] {
            let mut config = base.clone();
            config["entitlement_max_age_seconds"] = value;
            assert!(serde_json::from_value::<OidcConfig>(config).is_err());
        }
    }

    #[test]
    fn default_scopes_includes_openid() {
        let scopes = default_scopes();
        assert!(scopes.contains(&"openid".to_string()));
        assert!(scopes.contains(&"profile".to_string()));
        assert!(scopes.contains(&"email".to_string()));
    }

    #[test]
    fn default_claim_mapping_uses_standard_claims() {
        let claims = ClaimMappingConfig::default();
        assert_eq!(claims.email_claim, "email");
        assert_eq!(claims.name_claim, "name");
        assert_eq!(claims.roles_claim, "groups");
    }

    #[test]
    fn is_secure_context_detects_https() {
        let https_config = OidcConfig {
            issuer_url: "https://auth.example.com".to_string(),
            client_id: "test".to_string(),
            client_secret: "secret".to_string(),
            redirect_uri: "https://app.example.com/callback".to_string(),
            scopes: default_scopes(),
            claims: ClaimMappingConfig::default(),
            entitlement_max_age_seconds: default_entitlement_max_age_seconds(),
        };
        assert!(https_config.is_secure_context());

        let http_config = OidcConfig {
            issuer_url: "http://localhost:8080".to_string(),
            client_id: "test".to_string(),
            client_secret: "secret".to_string(),
            redirect_uri: "http://localhost:3445/callback".to_string(),
            scopes: default_scopes(),
            claims: ClaimMappingConfig::default(),
            entitlement_max_age_seconds: default_entitlement_max_age_seconds(),
        };
        assert!(!http_config.is_secure_context());
    }
}
