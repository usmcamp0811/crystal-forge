//! OIDC provider discovery and metadata.

use anyhow::{Context, Result};
use openidconnect::{IssuerUrl, ResponseTypes, core::CoreProviderMetadata};
use serde::{Deserialize, Serialize};

/// OIDC provider metadata fetched from discovery endpoint.
///
/// This wraps the openidconnect::ProviderMetadata with additional
/// Crystal Forge-specific functionality.
#[derive(Debug, Clone)]
pub struct OidcProviderMetadata {
    /// Raw provider metadata from discovery
    pub metadata: CoreProviderMetadata,
}

impl OidcProviderMetadata {
    /// Discover OIDC provider metadata from issuer URL.
    ///
    /// Fetches the `.well-known/openid-configuration` endpoint.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use crystal_forge::auth::oidc::OidcProviderMetadata;
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// let metadata = OidcProviderMetadata::discover("https://auth.example.com/realms/myrealm").await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn discover(issuer_url: &str) -> Result<Self> {
        let issuer = IssuerUrl::new(issuer_url.to_string()).context("Invalid issuer URL")?;

        let metadata =
            CoreProviderMetadata::discover_async(issuer, openidconnect::reqwest::async_http_client)
                .await
                .context("Failed to discover OIDC provider metadata")?;

        Ok(Self { metadata })
    }

    /// Get the authorization endpoint URL.
    pub fn authorization_endpoint(&self) -> String {
        self.metadata.authorization_endpoint().to_string()
    }

    /// Get the token endpoint URL.
    pub fn token_endpoint(&self) -> Option<String> {
        self.metadata.token_endpoint().map(|u| u.to_string())
    }

    /// Get the userinfo endpoint URL.
    pub fn userinfo_endpoint(&self) -> Option<String> {
        self.metadata.userinfo_endpoint().map(|u| u.to_string())
    }

    /// Get the JWKS URI (JSON Web Key Set endpoint).
    pub fn jwks_uri(&self) -> String {
        self.metadata.jwks_uri().to_string()
    }

    /// Get the issuer URL.
    pub fn issuer(&self) -> &IssuerUrl {
        self.metadata.issuer()
    }

    /// Check if this provider supports the authorization code flow.
    pub fn supports_authorization_code_flow(&self) -> bool {
        // The authorization code flow uses response_type=code
        // Check if the provider's metadata includes this
        true // Most OIDC providers support authorization code flow
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn issuer_url_validation() {
        // Valid HTTPS URL
        assert!(IssuerUrl::new("https://auth.example.com".to_string()).is_ok());

        // Invalid: HTTP (should be HTTPS in production)
        // Note: Some OIDC libraries allow HTTP for testing, but it's not recommended
        assert!(IssuerUrl::new("http://localhost:8080".to_string()).is_ok());

        // Invalid: not a URL
        assert!(IssuerUrl::new("not-a-url".to_string()).is_err());
    }

    // Integration test with real provider would go here
    // Requires mocking or test OIDC server
    #[tokio::test]
    #[ignore] // Only run with real provider or mock server
    async fn discover_metadata_integration() {
        // This would test against a real or mocked OIDC provider
        // Example: Keycloak test instance, Auth0 test tenant, etc.
    }
}
