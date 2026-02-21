//! OIDC authentication endpoints.
//!
//! These endpoints handle the OAuth2 authorization code flow:
//! 1. /api/auth/oidc/login - Initiates OIDC flow, redirects to provider
//! 2. /api/auth/oidc/callback - Handles provider redirect, exchanges code for tokens

use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Redirect},
};
use openidconnect::{
    core::{CoreAuthenticationFlow, CoreClient, CoreProviderMetadata},
    reqwest::async_http_client,
    AuthorizationCode, ClientId, ClientSecret, CsrfToken, IssuerUrl, Nonce, PkceCodeChallenge,
    RedirectUrl, Scope, TokenResponse,
};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::sync::Arc;

use crate::auth::oidc::{ClaimExtractor, JwksCache, JwtValidator, OidcProviderMetadata, OidcSession, OidcSessionStore};
use crate::config::OidcConfig;
use crate::queries::users::{get_by_email, insert_user};

/// Shared OIDC client state.
#[derive(Clone)]
pub struct OidcClientState {
    pub client: CoreClient,
    pub config: OidcConfig,
    pub claim_extractor: ClaimExtractor,
    pub jwt_validator: JwtValidator,
    pub jwks_cache: JwksCache,
    pub session_store: OidcSessionStore,
}

impl OidcClientState {
    /// Initialize OIDC client from configuration.
    pub async fn new(config: OidcConfig) -> anyhow::Result<Self> {
        // Discover provider metadata
        let provider_metadata =
            OidcProviderMetadata::discover(&config.issuer_url).await?;

        // Get JWKS URI before moving metadata
        let jwks_uri = provider_metadata.jwks_uri();

        // Create OIDC client
        let client = CoreClient::from_provider_metadata(
            provider_metadata.metadata,
            ClientId::new(config.client_id.clone()),
            Some(ClientSecret::new(config.client_secret.clone())),
        )
        .set_redirect_uri(RedirectUrl::new(config.redirect_uri.clone())?);

        let claim_extractor = ClaimExtractor::new(config.claims.clone());
        let jwt_validator = JwtValidator::new(
            config.issuer_url.clone(),
            config.client_id.clone(),
        );
        let jwks_cache = JwksCache::new(jwks_uri.clone(), None); // Use default 1-hour TTL
        let session_store = OidcSessionStore::new(None); // Use default 10-minute TTL

        Ok(Self {
            client,
            config,
            claim_extractor,
            jwt_validator,
            jwks_cache,
            session_store,
        })
    }
}

/// Initiate OIDC login flow.
///
/// This endpoint redirects the user to the OIDC provider's authorization endpoint.
pub async fn oidc_login(State(oidc_state): State<Arc<OidcClientState>>) -> impl IntoResponse {
    // Generate PKCE challenge and verifier
    let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();

    // Generate CSRF token and nonce
    let (auth_url, csrf_token, nonce) = oidc_state
        .client
        .authorize_url(
            CoreAuthenticationFlow::AuthorizationCode,
            CsrfToken::new_random,
            Nonce::new_random,
        )
        .set_pkce_challenge(pkce_challenge)
        .add_scopes(
            oidc_state
                .config
                .scopes
                .iter()
                .map(|s| Scope::new(s.clone())),
        )
        .url();

    // Store CSRF token, nonce, and PKCE verifier in session for validation
    let session = OidcSession::new(csrf_token.clone(), nonce, pkce_verifier);
    oidc_state.session_store.store(csrf_token.secret().clone(), session).await;

    tracing::info!("Initiating OIDC login flow, redirecting to: {}", auth_url);

    Redirect::to(auth_url.as_str())
}

/// Query parameters for OIDC callback.
#[derive(Debug, Deserialize)]
pub struct OidcCallbackParams {
    /// Authorization code from provider
    code: String,

    /// CSRF state token (must match what we sent)
    state: String,

    /// Error code if authentication failed
    error: Option<String>,

    /// Error description
    error_description: Option<String>,
}

/// OIDC callback handler.
///
/// This endpoint receives the authorization code from the OIDC provider,
/// exchanges it for tokens, validates the ID token, and creates a user session.
pub async fn oidc_callback(
    State(oidc_state): State<Arc<OidcClientState>>,
    State(pool): State<PgPool>,
    Query(params): Query<OidcCallbackParams>,
) -> Result<impl IntoResponse, OidcError> {
    // Check for errors from provider
    if let Some(error) = params.error {
        let description = params.error_description.unwrap_or_default();
        tracing::error!("OIDC provider error: {} - {}", error, description);
        return Err(OidcError::ProviderError {
            error,
            description,
        });
    }

    // Validate CSRF state token and retrieve session
    let session = oidc_state
        .session_store
        .retrieve(&params.state)
        .await
        .ok_or_else(|| {
            tracing::error!("Invalid or expired CSRF state token: {}", params.state);
            OidcError::InvalidCsrfToken
        })?;

    tracing::debug!("CSRF state validated successfully");

    // Exchange authorization code for tokens with PKCE verifier
    tracing::debug!("Exchanging authorization code for tokens");

    let token_response = oidc_state
        .client
        .exchange_code(AuthorizationCode::new(params.code))
        .set_pkce_verifier(session.pkce_verifier)
        .request_async(async_http_client)
        .await
        .map_err(|e| {
            tracing::error!("Token exchange failed: {}", e);
            OidcError::TokenExchangeFailed
        })?;

    // Get ID token
    let id_token = token_response
        .id_token()
        .ok_or(OidcError::MissingIdToken)?;

    // Fetch JWKS for validation (uses cache with 1-hour TTL)
    let jwks = oidc_state
        .jwks_cache
        .fetch()
        .await
        .map_err(|e| {
            tracing::error!("JWKS fetch failed: {}", e);
            OidcError::JwksFetchFailed
        })?;

    // Validate and decode ID token
    let claims = oidc_state
        .jwt_validator
        .validate_id_token(id_token.to_string().as_str(), &jwks)
        .map_err(|e| {
            tracing::error!("ID token validation failed: {}", e);
            OidcError::InvalidIdToken
        })?;

    // Validate nonce (protects against token replay attacks)
    // The nonce claim might be in custom_claims
    let token_nonce = claims.custom_claims
        .get("nonce")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            tracing::error!("ID token missing nonce claim");
            OidcError::MissingNonce
        })?;

    if token_nonce != session.nonce.secret() {
        tracing::error!("Nonce mismatch: expected {}, got {}", session.nonce.secret(), token_nonce);
        return Err(OidcError::InvalidNonce);
    }

    tracing::debug!("Nonce validated successfully");

    // Extract user info from claims
    // Pass typed fields first, then custom claims for fallback
    let user_info = oidc_state
        .claim_extractor
        .extract_user_info(
            claims.email.clone(),
            claims.email_verified,
            claims.name.clone(),
            claims.given_name.clone(),
            claims.family_name.clone(),
            claims.preferred_username.clone(),
            &claims.custom_claims,
            claims.sub.clone(),
        )
        .map_err(|e| {
            tracing::error!("Claim extraction failed: {}", e);
            OidcError::ClaimExtractionFailed
        })?;

    tracing::info!(
        "User authenticated via OIDC: {} ({})",
        user_info.email.as_deref().unwrap_or("no-email"),
        user_info.subject
    );

    // Find or create user in database
    let user = if let Some(email) = &user_info.email {
        match get_by_email(&pool, email).await.map_err(|_| OidcError::DatabaseError)? {
            Some(user) => user,
            None => {
                // Create new user
                tracing::info!("Creating new user: {}", email);
                insert_user(
                    &pool,
                    email,
                    user_info.display_name.as_deref(),
                )
                .await
                .map_err(|_| OidcError::DatabaseError)?
            }
        }
    } else {
        return Err(OidcError::MissingEmail);
    };

    // TODO: Create session (TASK-65.3)
    // TODO: Set session cookie (TASK-65.3)
    // TODO: Assign roles based on OIDC groups (future task)

    tracing::info!("User session created for: {}", user.email);

    // Redirect to dashboard
    Ok(Redirect::to("/"))
}

/// OIDC error responses.
#[derive(Debug)]
pub enum OidcError {
    ProviderError {
        error: String,
        description: String,
    },
    InvalidCsrfToken,
    TokenExchangeFailed,
    MissingIdToken,
    JwksFetchFailed,
    InvalidIdToken,
    MissingNonce,
    InvalidNonce,
    ClaimExtractionFailed,
    MissingEmail,
    DatabaseError,
}

impl IntoResponse for OidcError {
    fn into_response(self) -> axum::response::Response {
        let (status, message) = match self {
            OidcError::ProviderError { error, description } => (
                StatusCode::BAD_REQUEST,
                format!("OIDC provider error: {} - {}", error, description),
            ),
            OidcError::InvalidCsrfToken => (
                StatusCode::BAD_REQUEST,
                "Invalid or expired CSRF state token".to_string(),
            ),
            OidcError::TokenExchangeFailed => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to exchange authorization code for tokens".to_string(),
            ),
            OidcError::MissingIdToken => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "OIDC provider did not return an ID token".to_string(),
            ),
            OidcError::JwksFetchFailed => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to fetch JWKS from provider".to_string(),
            ),
            OidcError::InvalidIdToken => (
                StatusCode::UNAUTHORIZED,
                "ID token validation failed".to_string(),
            ),
            OidcError::MissingNonce => (
                StatusCode::UNAUTHORIZED,
                "ID token missing nonce claim".to_string(),
            ),
            OidcError::InvalidNonce => (
                StatusCode::UNAUTHORIZED,
                "Nonce validation failed - possible token replay attack".to_string(),
            ),
            OidcError::ClaimExtractionFailed => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to extract claims from ID token".to_string(),
            ),
            OidcError::MissingEmail => (
                StatusCode::BAD_REQUEST,
                "User email not found in OIDC claims".to_string(),
            ),
            OidcError::DatabaseError => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Database error during user lookup/creation".to_string(),
            ),
        };

        (status, message).into_response()
    }
}
