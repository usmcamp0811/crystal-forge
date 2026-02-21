//! OIDC provider configuration.

use serde::{Deserialize, Serialize};

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
    pub fn from_env() -> anyhow::Result<Self> {
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
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
