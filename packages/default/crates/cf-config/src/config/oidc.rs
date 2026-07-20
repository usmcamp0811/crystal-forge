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

    #[test]
    fn is_secure_context_detects_https() {
        let https_config = OidcConfig {
            issuer_url: "https://auth.example.com".to_string(),
            client_id: "test".to_string(),
            client_secret: "secret".to_string(),
            redirect_uri: "https://app.example.com/callback".to_string(),
            scopes: default_scopes(),
            claims: ClaimMappingConfig::default(),
        };
        assert!(https_config.is_secure_context());

        let http_config = OidcConfig {
            issuer_url: "http://localhost:8080".to_string(),
            client_id: "test".to_string(),
            client_secret: "secret".to_string(),
            redirect_uri: "http://localhost:3445/callback".to_string(),
            scopes: default_scopes(),
            claims: ClaimMappingConfig::default(),
        };
        assert!(!http_config.is_secure_context());
    }
}
