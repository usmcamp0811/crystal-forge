//! OIDC authentication endpoints.
//!
//! These endpoints handle the OAuth2 authorization code flow:
//! 1. /api/auth/oidc/login - Initiates OIDC flow, redirects to provider
//! 2. /api/auth/oidc/callback - Handles provider redirect, exchanges code for tokens

use axum::{
    extract::{ConnectInfo, Extension, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use openidconnect::{
    AuthorizationCode, ClientId, ClientSecret, CsrfToken, Nonce, PkceCodeChallenge, RedirectUrl,
    Scope, TokenResponse,
    core::{CoreAuthenticationFlow, CoreClient},
    reqwest::async_http_client,
};
use serde::Deserialize;
use sqlx::PgPool;
use std::collections::BTreeSet;
use std::net::SocketAddr;
use std::sync::Arc;

use crate::auth::oidc::{
    ClaimExtractor, JwksCache, JwtValidator, OidcProviderMetadata, OidcSession, OidcSessionStore,
};
use crate::config::OidcConfig;
use crate::handlers::api::auth_session::establish_user_session;
use crate::models::auth_identity::AuthRole;
use crate::queries::auth_identity::{
    AuthIdentityRepository, OidcMappingMatchRow, assign_role_to_user,
    clear_user_environment_memberships, clear_user_role_assignments,
    create_user_and_bind_external_identity, get_environment_ids_by_names, get_oidc_mapping_matches,
    get_user_by_id, get_user_roles, insert_user_environment_membership,
};

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
        let provider_metadata = OidcProviderMetadata::discover(&config.issuer_url).await?;

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
        let jwt_validator = JwtValidator::new(config.issuer_url.clone(), config.client_id.clone());
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
/// Sets a secure cookie binding the OIDC state to this browser session.
pub async fn oidc_login(
    Extension(oidc_state): Extension<Arc<OidcClientState>>,
) -> impl IntoResponse {
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
    let state_value = csrf_token.secret().clone();
    oidc_state
        .session_store
        .store(state_value.clone(), session)
        .await;

    tracing::info!("Initiating OIDC login flow, redirecting to: {}", auth_url);

    // SECURITY: Bind state to browser session via secure cookie
    // This prevents login CSRF and account confusion attacks
    let is_secure = oidc_state.config.is_secure_context();
    let cookie = create_oidc_state_cookie(&state_value, is_secure);

    // Return redirect with Set-Cookie header
    Response::builder()
        .status(StatusCode::FOUND)
        .header(header::LOCATION, auth_url.as_str())
        .header(header::SET_COOKIE, cookie)
        .body(axum::body::Body::empty())
        .unwrap()
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
///
/// **Security**: Validates that the state parameter matches the session cookie
/// to prevent login CSRF and account confusion attacks.
pub async fn oidc_callback(
    Extension(oidc_state): Extension<Arc<OidcClientState>>,
    State(pool): State<PgPool>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Query(params): Query<OidcCallbackParams>,
) -> Result<impl IntoResponse, OidcError> {
    // Check for errors from provider
    if let Some(error) = params.error {
        let description = params.error_description.unwrap_or_default();
        tracing::error!("OIDC provider error: {} - {}", error, description);
        return Err(OidcError::ProviderError { error, description });
    }

    // SECURITY: Validate state parameter matches session cookie
    // This binds the OAuth2 flow to the browser session, preventing:
    // - Login CSRF attacks (attacker can't force victim to use attacker's account)
    // - Account confusion (multiple concurrent logins don't mix state)
    let cookie_state = extract_oidc_state_cookie(&headers).ok_or_else(|| {
        tracing::error!("Missing or invalid __Host-oidc-state cookie");
        OidcError::MissingStateCookie
    })?;

    if cookie_state != params.state {
        // Don't log raw state tokens (sensitive values)
        tracing::error!(
            "State mismatch: cookie_len={} param_len={} match=false",
            cookie_state.len(),
            params.state.len()
        );
        return Err(OidcError::StateMismatch);
    }

    tracing::debug!("State cookie validated successfully");

    // Validate CSRF state token and retrieve session
    let session = oidc_state
        .session_store
        .retrieve(&params.state)
        .await
        .ok_or_else(|| {
            // Don't log raw state token (sensitive value)
            tracing::error!(
                "Invalid or expired CSRF state token (len={})",
                params.state.len()
            );
            OidcError::InvalidCsrfToken
        })?;

    tracing::debug!("CSRF state token validated successfully");

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
    let id_token = token_response.id_token().ok_or(OidcError::MissingIdToken)?;

    // Fetch JWKS for validation (uses cache with 1-hour TTL)
    let jwks = oidc_state.jwks_cache.fetch().await.map_err(|e| {
        tracing::error!("JWKS fetch failed: {}", e);
        OidcError::JwksFetchFailed
    })?;

    // Validate and decode ID token
    // If validation fails due to missing key (kid not found), retry with fresh JWKS
    // This handles provider key rotation scenarios
    let claims = match oidc_state
        .jwt_validator
        .validate_id_token(id_token.to_string().as_str(), &jwks)
    {
        Ok(claims) => claims,
        Err(e) if e.to_string().contains("No matching key found in JWKS") => {
            tracing::warn!(
                "Key not found in cached JWKS, force refreshing (possible key rotation)"
            );

            // Force refresh JWKS cache (bypass TTL) and retry once
            let fresh_jwks = oidc_state.jwks_cache.force_refresh().await.map_err(|e| {
                tracing::error!("JWKS force refresh failed: {}", e);
                OidcError::JwksFetchFailed
            })?;

            oidc_state
                .jwt_validator
                .validate_id_token(id_token.to_string().as_str(), &fresh_jwks)
                .map_err(|e| {
                    tracing::error!("ID token validation failed after JWKS refresh: {}", e);
                    OidcError::InvalidIdToken
                })?
        }
        Err(e) => {
            tracing::error!("ID token validation failed: {}", e);
            return Err(OidcError::InvalidIdToken);
        }
    };

    // Validate nonce (protects against token replay attacks)
    // The nonce claim might be in custom_claims
    let token_nonce = claims
        .custom_claims
        .get("nonce")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            tracing::error!("ID token missing nonce claim");
            OidcError::MissingNonce
        })?;

    if token_nonce != session.nonce.secret() {
        // Don't log raw nonce values (sensitive)
        tracing::error!(
            "Nonce mismatch: expected_len={} token_len={} match=false",
            session.nonce.secret().len(),
            token_nonce.len()
        );
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

    // SECURITY: Require email_verified=true before account linking/creation
    // Prevents account takeover on providers that don't verify email addresses
    let email = user_info.email.as_ref().ok_or(OidcError::MissingEmail)?;

    if !user_info.email_verified {
        tracing::error!(
            "Email not verified by provider (email_verified=false) - rejecting authentication"
        );
        return Err(OidcError::UnverifiedEmail);
    }

    tracing::debug!("Email verified by provider");

    // SECURITY: Bind account using stable OIDC identity (sub + iss), NOT email
    // This prevents account takeover when emails are reassigned at the provider level
    //
    // Attack scenarios prevented:
    // 1. User alice@example.com authenticates → Creates account A
    // 2. Alice leaves company, email reassigned to Bob at provider
    // 3. Bob authenticates with alice@example.com
    //    - Email-based: Bob gets account A (Alice's account) ❌
    //    - Subject-based: Bob gets new account B (correct) ✅
    //
    // Using (provider_key, subject, tenant_discriminator) as unique identity
    let auth_repo = AuthIdentityRepository::new(&pool);
    let provider_key = "oidc"; // Generic OIDC provider
    let subject = &user_info.subject;
    let tenant_discriminator = Some(claims.iss.as_str()); // Use issuer as tenant

    // Find existing identity binding
    let external_identity = auth_repo
        .find_external_identity(provider_key, subject, tenant_discriminator)
        .await
        .map_err(|e| {
            tracing::error!("Failed to find external identity: {}", e);
            OidcError::DatabaseError
        })?;

    let user = match external_identity {
        Some(identity) => {
            // Existing OIDC identity found - load the linked user
            tracing::debug!(
                "Existing OIDC identity found: provider={} subject={} user_id={}",
                provider_key,
                subject,
                identity.user_id
            );

            get_user_by_id(&pool, identity.user_id).await.map_err(|e| {
                tracing::error!("Failed to load user for identity: {}", e);
                OidcError::DatabaseError
            })?
        }
        None => {
            // New OIDC identity - create user and bind identity atomically
            tracing::info!("New OIDC identity, creating user: email={}", email);

            let claims_json = serde_json::to_value(&user_info.custom_claims)
                .unwrap_or_else(|_| serde_json::json!({}));
            let created = create_user_and_bind_external_identity(
                &pool,
                email,
                user_info.display_name.as_deref(),
                provider_key,
                subject,
                tenant_discriminator,
                claims_json,
            )
            .await
            .map_err(|e| {
                tracing::error!("Failed to create user/identity binding: {}", e);
                OidcError::DatabaseError
            })?;

            tracing::info!(
                "Created user + OIDC identity binding atomically: user_id={} provider={} subject={}",
                created.id,
                provider_key,
                subject
            );

            created
        }
    };

    let user_agent = headers
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .map(ToString::to_string);

    let ip_address = Some(addr.ip().to_string());

    let groups = normalize_oidc_groups(&user_info.roles);

    let mapping_rows = get_oidc_mapping_matches(&pool, &groups)
        .await
        .map_err(|e| {
            tracing::error!("Failed to load OIDC group mappings: {}", e);
            OidcError::DatabaseError
        })?;

    let mapped_role = derive_highest_role(mapping_rows.iter().filter_map(|row| row.role));
    let mapped_environments = collect_mapped_environments(&mapping_rows);

    let existing_roles = get_user_roles(&pool, user.id).await.map_err(|e| {
        tracing::error!("Failed to check user roles: {}", e);
        OidcError::DatabaseError
    })?;

    if let Some(role) = mapped_role {
        clear_user_role_assignments(&pool, user.id)
            .await
            .map_err(|e| {
                tracing::error!("Failed to reset user role assignments: {}", e);
                OidcError::DatabaseError
            })?;

        assign_role_to_user(&pool, user.id, role, None)
            .await
            .map_err(|e| {
                tracing::error!("Failed to assign mapped role: {}", e);
                OidcError::DatabaseError
            })?;
    } else if existing_roles.is_empty() {
        assign_role_to_user(&pool, user.id, AuthRole::Viewer, None)
            .await
            .map_err(|e| {
                tracing::error!("Failed to assign default role: {}", e);
                OidcError::DatabaseError
            })?;
    }

    let mut apply_environment_mappings = false;
    let mut resolved_environment_ids = Vec::new();

    if !mapping_rows.is_empty() && !mapped_environments.is_empty() {
        let environment_ids = get_environment_ids_by_names(&pool, &mapped_environments)
            .await
            .map_err(|e| {
                tracing::error!("Failed to resolve mapped environments: {}", e);
                OidcError::DatabaseError
            })?;

        match evaluate_environment_mapping_apply(
            mapping_rows.len(),
            mapped_environments.len(),
            environment_ids.len(),
        ) {
            EnvironmentMappingApply::Apply => {
                apply_environment_mappings = true;
                resolved_environment_ids = environment_ids;
            }
            EnvironmentMappingApply::SkipNoMappings
            | EnvironmentMappingApply::SkipNoMappedEnvironments => {}
            EnvironmentMappingApply::SkipUnknownMappedEnvironments => {
                tracing::warn!(
                    "OIDC mapped environments include unknown names; preserving existing memberships for user_id={}",
                    user.id
                );
            }
        }
    }

    if apply_environment_mappings {
        clear_user_environment_memberships(&pool, user.id)
            .await
            .map_err(|e| {
                tracing::error!("Failed to reset environment memberships: {}", e);
                OidcError::DatabaseError
            })?;

        for environment_id in resolved_environment_ids {
            insert_user_environment_membership(&pool, user.id, environment_id)
                .await
                .map_err(|e| {
                    tracing::error!("Failed to assign mapped environment: {}", e);
                    OidcError::DatabaseError
                })?;
        }
    } else if mapping_rows.is_empty() {
        tracing::debug!(
            "No OIDC mappings matched groups; preserving existing environment memberships for user_id={}",
            user.id
        );
    } else if mapped_environments.is_empty() {
        tracing::warn!(
            "OIDC mappings resolved without environments; preserving existing memberships for user_id={}",
            user.id
        );
    }

    let session_cookies = establish_user_session(&pool, user.id, user_agent, ip_address)
        .await
        .map_err(|_| OidcError::SessionCreationFailed)?;
    // TODO: Update external_identity claims on each login (keep profile fresh)

    tracing::info!(
        "User authenticated via OIDC: user_id={} email={}",
        user.id,
        user.email
    );

    // Redirect to dashboard with authenticated session cookies.
    let mut response = Response::builder()
        .status(StatusCode::FOUND)
        .header(header::LOCATION, "/")
        .body(axum::body::Body::empty())
        .unwrap();

    let is_secure = oidc_state.config.is_secure_context();
    response.headers_mut().append(
        header::SET_COOKIE,
        HeaderValue::from_static(delete_oidc_state_cookie(is_secure)),
    );
    response
        .headers_mut()
        .append(header::SET_COOKIE, session_cookies.session_cookie);
    response
        .headers_mut()
        .append(header::SET_COOKIE, session_cookies.csrf_cookie);

    Ok(response)
}

fn derive_highest_role(roles: impl Iterator<Item = AuthRole>) -> Option<AuthRole> {
    let mut has_operator = false;
    let mut has_viewer = false;
    for role in roles {
        match role {
            AuthRole::Admin => return Some(AuthRole::Admin),
            AuthRole::Operator => has_operator = true,
            AuthRole::Viewer => has_viewer = true,
        }
    }

    if has_operator {
        Some(AuthRole::Operator)
    } else if has_viewer {
        Some(AuthRole::Viewer)
    } else {
        None
    }
}

fn normalize_oidc_groups(values: &[String]) -> Vec<String> {
    values
        .iter()
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
        .collect()
}

fn collect_mapped_environments(rows: &[OidcMappingMatchRow]) -> Vec<String> {
    rows.iter()
        .flat_map(|row| row.environments.iter())
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EnvironmentMappingApply {
    Apply,
    SkipNoMappings,
    SkipNoMappedEnvironments,
    SkipUnknownMappedEnvironments,
}

fn evaluate_environment_mapping_apply(
    mapping_row_count: usize,
    mapped_environment_count: usize,
    resolved_environment_count: usize,
) -> EnvironmentMappingApply {
    if mapping_row_count == 0 {
        return EnvironmentMappingApply::SkipNoMappings;
    }

    if mapped_environment_count == 0 {
        return EnvironmentMappingApply::SkipNoMappedEnvironments;
    }

    if resolved_environment_count != mapped_environment_count {
        return EnvironmentMappingApply::SkipUnknownMappedEnvironments;
    }

    EnvironmentMappingApply::Apply
}

/// Generate the OIDC state cookie name based on security context.
///
/// In production (HTTPS), uses `__Host-oidc-state` prefix for enhanced security.
/// In development (HTTP), uses `oidc-state` to avoid browser rejection.
fn oidc_state_cookie_name(is_secure: bool) -> &'static str {
    if is_secure {
        "__Host-oidc-state"
    } else {
        "oidc-state"
    }
}

/// Create an OIDC state cookie string with appropriate security attributes.
///
/// **Security Note**: In production (HTTPS), uses:
/// - `__Host-` prefix (requires HTTPS, Path=/, no Domain)
/// - `Secure` flag
/// - `HttpOnly` flag
/// - `SameSite=Lax`
///
/// In development (HTTP), uses same attributes except:
/// - Regular cookie name without `__Host-` prefix
/// - No `Secure` flag (browsers reject Secure cookies on HTTP)
fn create_oidc_state_cookie(state_value: &str, is_secure: bool) -> String {
    let cookie_name = oidc_state_cookie_name(is_secure);
    if is_secure {
        format!(
            "{}={}; Path=/; Secure; HttpOnly; SameSite=Lax; Max-Age=600",
            cookie_name, state_value
        )
    } else {
        format!(
            "{}={}; Path=/; HttpOnly; SameSite=Lax; Max-Age=600",
            cookie_name, state_value
        )
    }
}

/// Create a cookie deletion string for the OIDC state cookie.
fn delete_oidc_state_cookie(is_secure: bool) -> &'static str {
    if is_secure {
        "__Host-oidc-state=; Path=/; Secure; HttpOnly; SameSite=Lax; Max-Age=0; Expires=Thu, 01 Jan 1970 00:00:00 GMT"
    } else {
        "oidc-state=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0; Expires=Thu, 01 Jan 1970 00:00:00 GMT"
    }
}

/// Extract OIDC state from cookie (tries both secure and non-secure names).
fn extract_oidc_state_cookie(headers: &HeaderMap) -> Option<String> {
    let cookie_header = headers.get(header::COOKIE)?.to_str().ok()?;

    // Try both cookie names (secure and non-secure)
    for prefix in ["__Host-oidc-state=", "oidc-state="] {
        if let Some(value) = cookie_header
            .split(';')
            .find_map(|cookie| cookie.trim().strip_prefix(prefix).map(|v| v.to_string()))
        {
            return Some(value);
        }
    }

    None
}

/// OIDC error responses.
#[derive(Debug)]
pub enum OidcError {
    ProviderError { error: String, description: String },
    MissingStateCookie,
    StateMismatch,
    InvalidCsrfToken,
    TokenExchangeFailed,
    MissingIdToken,
    JwksFetchFailed,
    InvalidIdToken,
    MissingNonce,
    InvalidNonce,
    ClaimExtractionFailed,
    MissingEmail,
    UnverifiedEmail,
    RoleAssignmentFailed,
    SessionCreationFailed,
    DatabaseError,
}

impl IntoResponse for OidcError {
    fn into_response(self) -> axum::response::Response {
        let (status, message) = match self {
            OidcError::ProviderError { error, description } => (
                StatusCode::BAD_REQUEST,
                format!("OIDC provider error: {} - {}", error, description),
            ),
            OidcError::MissingStateCookie => (
                StatusCode::BAD_REQUEST,
                "Missing OIDC state cookie - login session not found".to_string(),
            ),
            OidcError::StateMismatch => (
                StatusCode::BAD_REQUEST,
                "State mismatch - possible login CSRF attack".to_string(),
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
            OidcError::UnverifiedEmail => (
                StatusCode::FORBIDDEN,
                "Email not verified by OIDC provider - cannot create/link account".to_string(),
            ),
            OidcError::RoleAssignmentFailed => (
                StatusCode::FORBIDDEN,
                "No valid OIDC role mapping found for user".to_string(),
            ),
            OidcError::SessionCreationFailed => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Authenticated but failed to create session".to_string(),
            ),
            OidcError::DatabaseError => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Database error during user lookup/creation".to_string(),
            ),
        };

        (status, message).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derive_highest_role_prefers_admin_then_operator_then_viewer() {
        assert_eq!(
            derive_highest_role(vec![AuthRole::Viewer, AuthRole::Operator].into_iter()),
            Some(AuthRole::Operator)
        );
        assert_eq!(
            derive_highest_role(vec![AuthRole::Viewer, AuthRole::Admin].into_iter()),
            Some(AuthRole::Admin)
        );
        assert_eq!(derive_highest_role(vec![].into_iter()), None);
    }

    #[test]
    fn normalize_oidc_groups_trims_and_lowercases() {
        let groups = normalize_oidc_groups(&[
            " Team-Admins ".to_string(),
            "".to_string(),
            "Platform/Ops".to_string(),
        ]);

        assert_eq!(
            groups,
            vec!["team-admins".to_string(), "platform/ops".to_string()]
        );
    }

    #[test]
    fn collect_mapped_environments_deduplicates_normalized_values() {
        let rows = vec![
            OidcMappingMatchRow {
                role: Some(AuthRole::Viewer),
                environments: vec!["Prod".to_string(), " staging ".to_string()],
            },
            OidcMappingMatchRow {
                role: None,
                environments: vec!["prod".to_string(), "".to_string()],
            },
        ];

        let environments = collect_mapped_environments(&rows);
        assert_eq!(
            environments,
            vec!["prod".to_string(), "staging".to_string()]
        );
    }

    #[test]
    fn environment_mapping_apply_requires_actionable_and_resolved_mappings() {
        assert_eq!(
            evaluate_environment_mapping_apply(0, 2, 2),
            EnvironmentMappingApply::SkipNoMappings
        );
        assert_eq!(
            evaluate_environment_mapping_apply(2, 0, 0),
            EnvironmentMappingApply::SkipNoMappedEnvironments
        );
        assert_eq!(
            evaluate_environment_mapping_apply(2, 2, 1),
            EnvironmentMappingApply::SkipUnknownMappedEnvironments
        );
        assert_eq!(
            evaluate_environment_mapping_apply(2, 2, 2),
            EnvironmentMappingApply::Apply
        );
    }

    #[test]
    fn oidc_state_cookie_uses_host_prefix_in_secure_context() {
        assert_eq!(oidc_state_cookie_name(true), "__Host-oidc-state");
        assert_eq!(oidc_state_cookie_name(false), "oidc-state");
    }

    #[test]
    fn oidc_state_cookie_includes_secure_flag_in_secure_context() {
        let secure_cookie = create_oidc_state_cookie("test-state", true);
        assert!(secure_cookie.contains("__Host-oidc-state="));
        assert!(secure_cookie.contains("Secure"));
        assert!(secure_cookie.contains("HttpOnly"));
        assert!(secure_cookie.contains("SameSite=Lax"));
    }

    #[test]
    fn oidc_state_cookie_omits_secure_flag_in_insecure_context() {
        let insecure_cookie = create_oidc_state_cookie("test-state", false);
        assert!(insecure_cookie.contains("oidc-state="));
        assert!(!insecure_cookie.contains("__Host-"));
        assert!(!insecure_cookie.contains("Secure"));
        assert!(insecure_cookie.contains("HttpOnly"));
        assert!(insecure_cookie.contains("SameSite=Lax"));
    }

    #[test]
    fn delete_oidc_state_cookie_matches_context() {
        let secure_delete = delete_oidc_state_cookie(true);
        assert!(secure_delete.contains("__Host-oidc-state="));
        assert!(secure_delete.contains("Max-Age=0"));

        let insecure_delete = delete_oidc_state_cookie(false);
        assert!(insecure_delete.contains("oidc-state="));
        assert!(!insecure_delete.contains("__Host-"));
        assert!(insecure_delete.contains("Max-Age=0"));
    }

    #[test]
    fn extract_oidc_state_cookie_handles_both_formats() {
        use axum::http::HeaderMap;

        // Test secure cookie
        let mut headers = HeaderMap::new();
        headers.insert(
            header::COOKIE,
            HeaderValue::from_static("__Host-oidc-state=secure-value; other=cookie"),
        );
        assert_eq!(
            extract_oidc_state_cookie(&headers),
            Some("secure-value".to_string())
        );

        // Test non-secure cookie
        let mut headers = HeaderMap::new();
        headers.insert(
            header::COOKIE,
            HeaderValue::from_static("oidc-state=insecure-value; other=cookie"),
        );
        assert_eq!(
            extract_oidc_state_cookie(&headers),
            Some("insecure-value".to_string())
        );

        // Test missing cookie
        let headers = HeaderMap::new();
        assert_eq!(extract_oidc_state_cookie(&headers), None);
    }
}
