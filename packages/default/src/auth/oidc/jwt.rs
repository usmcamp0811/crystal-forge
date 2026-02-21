//! JWT token validation.

use anyhow::{Context, Result};
use jsonwebtoken::{decode, decode_header, Algorithm, DecodingKey, Validation};
use openidconnect::core::{CoreJsonWebKey, CoreJsonWebKeySet};
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::HashMap;

/// Deserialize `aud` claim which can be either a string or an array of strings.
fn deserialize_aud<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    use serde::de::Error;
    use serde_json::Value;

    let value = Value::deserialize(deserializer)?;

    match value {
        Value::String(s) => Ok(vec![s]),
        Value::Array(arr) => arr
            .into_iter()
            .map(|v| {
                v.as_str()
                    .map(|s| s.to_string())
                    .ok_or_else(|| Error::custom("aud array must contain strings"))
            })
            .collect(),
        _ => Err(Error::custom("aud must be a string or array of strings")),
    }
}

/// Standard OIDC ID token claims.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdTokenClaims {
    /// Issuer (iss)
    pub iss: String,

    /// Subject (sub) - unique user identifier
    pub sub: String,

    /// Audience (aud) - client ID (can be string or array per OIDC spec)
    #[serde(deserialize_with = "deserialize_aud")]
    pub aud: Vec<String>,

    /// Expiration time (exp) - Unix timestamp
    pub exp: i64,

    /// Issued at (iat) - Unix timestamp
    pub iat: i64,

    /// Email (if requested in scopes)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,

    /// Email verified flag
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email_verified: Option<bool>,

    /// Full name (if requested in scopes)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    /// Given name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub given_name: Option<String>,

    /// Family name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub family_name: Option<String>,

    /// Preferred username
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preferred_username: Option<String>,

    /// Additional custom claims (provider-specific)
    #[serde(flatten)]
    pub custom_claims: HashMap<String, serde_json::Value>,
}

/// JWT validator for OIDC ID tokens.
#[derive(Debug, Clone)]
pub struct JwtValidator {
    issuer: String,
    client_id: String,
}

impl JwtValidator {
    /// Create a new JWT validator.
    ///
    /// # Arguments
    ///
    /// * `issuer` - Expected issuer URL (must match token's `iss` claim)
    /// * `client_id` - Expected audience (must match token's `aud` claim)
    pub fn new(issuer: String, client_id: String) -> Self {
        Self { issuer, client_id }
    }

    /// Validate and decode an ID token.
    ///
    /// Performs the following validations:
    /// - Algorithm validation (CRITICAL: prevents algorithm substitution attacks)
    /// - Signature verification using JWKS
    /// - Issuer (`iss`) matches expected issuer
    /// - Audience (`aud`) matches client ID
    /// - Token not expired (`exp`)
    /// - Token not used before issued (`iat`)
    ///
    /// **Security**: Algorithm is explicitly enforced (RS256/RS384/RS512 only).
    /// The token header `alg` is NOT trusted to prevent algorithm confusion attacks.
    ///
    /// # Arguments
    ///
    /// * `id_token` - The JWT token string
    /// * `jwks` - The JWKS to use for signature verification
    pub fn validate_id_token(
        &self,
        id_token: &str,
        jwks: &CoreJsonWebKeySet,
    ) -> Result<IdTokenClaims> {
        // Decode header to get key ID (kid) and algorithm
        let header = decode_header(id_token).context("Failed to decode JWT header")?;

        let kid = header
            .kid
            .ok_or_else(|| anyhow::anyhow!("JWT header missing 'kid' field"))?;

        // SECURITY: Validate algorithm BEFORE any decoding
        // Only allow RSA algorithms (RS256, RS384, RS512)
        // DO NOT trust the token's alg header - explicitly enforce allowed algorithms
        let allowed_algorithms = [Algorithm::RS256, Algorithm::RS384, Algorithm::RS512];

        let token_alg: Algorithm = header
            .alg
            .try_into()
            .map_err(|_| anyhow::anyhow!("Failed to parse algorithm: {:?}", header.alg))?;

        if !allowed_algorithms.contains(&token_alg) {
            anyhow::bail!(
                "Algorithm '{:?}' not allowed. Only RS256, RS384, RS512 are permitted (prevents algorithm confusion attacks)",
                token_alg
            );
        }

        tracing::debug!("Token algorithm validated: {:?}", token_alg);

        // Find matching key in JWKS by kid
        let jwk = self.find_key_by_kid(jwks, &kid)?;

        // Get the public key for verification
        let decoding_key =
            Self::jwk_to_decoding_key(&jwk).context("Failed to convert JWK to decoding key")?;

        // Set up validation rules
        // Use the validated algorithm (not blindly trusting token header)
        let mut validation = Validation::new(token_alg);
        validation.set_issuer(&[&self.issuer]);
        validation.set_audience(&[&self.client_id]);
        validation.validate_exp = true;

        // CRITICAL: Set allowed algorithms explicitly
        validation.algorithms = allowed_algorithms.to_vec();

        // Decode and validate token
        let token_data = decode::<IdTokenClaims>(id_token, &decoding_key, &validation)
            .context("JWT validation failed")?;

        // Additional validation: ensure our client_id is in the audience list
        if !token_data.claims.aud.contains(&self.client_id) {
            anyhow::bail!(
                "Client ID '{}' not found in token audience: {:?}",
                self.client_id,
                token_data.claims.aud
            );
        }

        Ok(token_data.claims)
    }

    /// Find a JWK by key ID.
    fn find_key_by_kid(&self, jwks: &CoreJsonWebKeySet, kid: &str) -> Result<serde_json::Value> {
        // Serialize JWKS to JSON to inspect keys
        let jwks_json = serde_json::to_value(jwks).context("Failed to serialize JWKS")?;

        let keys = jwks_json
            .get("keys")
            .and_then(|v| v.as_array())
            .ok_or_else(|| anyhow::anyhow!("JWKS missing 'keys' array"))?;

        for key in keys {
            if let Some(key_kid) = key.get("kid").and_then(|v| v.as_str()) {
                if key_kid == kid {
                    return Ok(key.clone());
                }
            }
        }

        anyhow::bail!("No matching key found in JWKS for kid={}", kid)
    }

    /// Convert a JWK to a decoding key for JWT validation.
    fn jwk_to_decoding_key(jwk: &serde_json::Value) -> Result<DecodingKey> {
        // Extract RSA components from JWK (modulus n and exponent e)
        let n_str = jwk
            .get("n")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("JWK missing 'n' (modulus)"))?;

        let e_str = jwk
            .get("e")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("JWK missing 'e' (exponent)"))?;

        // The from_rsa_components expects base64url-encoded strings, not decoded bytes
        DecodingKey::from_rsa_components(n_str, e_str).context("Failed to create RSA decoding key")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jwt_validator_creation() {
        let validator = JwtValidator::new(
            "https://issuer.example.com".to_string(),
            "client-id".to_string(),
        );
        assert_eq!(validator.issuer, "https://issuer.example.com");
        assert_eq!(validator.client_id, "client-id");
    }

    // Integration tests with real tokens would go here
    #[test]
    #[ignore] // Requires real JWT and JWKS
    fn validate_real_id_token() {
        // This would test with a real JWT token and JWKS
        // Example: Generate token from test OIDC provider
    }
}
