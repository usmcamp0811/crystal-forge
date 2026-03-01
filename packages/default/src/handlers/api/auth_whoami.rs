//! Authentication context endpoint.
//!
//! Provides the `/api/auth/whoami` endpoint which returns the current
//! authentication state, user information, and assigned roles.

use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use sqlx::PgPool;

use crate::{
    api::models::{AuthContext, AuthMode, AuthUser, Role},
    auth::session::{SESSION_COOKIE_NAME, extract_cookie, hash_token},
    queries::auth_identity::{get_session_by_token_hash, get_user_roles},
    queries::users::get_by_id,
};

/// Get current authentication context.
///
/// Returns authentication state including user info, roles, and auth mode.
/// This endpoint is publicly accessible (no auth required) so the UI can
/// determine whether the user is authenticated.
pub async fn whoami(State(pool): State<PgPool>, headers: HeaderMap) -> impl IntoResponse {
    let auth_mode = detect_auth_mode();

    // Try to extract session cookie
    let session_token = extract_cookie(&headers, SESSION_COOKIE_NAME);

    if let Some(token) = session_token {
        let token_hash = hash_token(&token);

        // Look up session
        match get_session_by_token_hash(&pool, &token_hash).await {
            Ok(Some(session)) if !session.is_expired() && !session.is_invalidated() => {
                // Valid session found - fetch user and roles
                match get_by_id(&pool, session.user_id).await {
                    Ok(Some(user)) => {
                        let roles = get_user_roles(&pool, user.id)
                            .await
                            .unwrap_or_default()
                            .into_iter()
                            .map(|r| match r.role {
                                crate::models::auth_identity::AuthRole::Admin => Role::Admin,
                                crate::models::auth_identity::AuthRole::Operator => Role::Operator,
                                crate::models::auth_identity::AuthRole::Viewer => Role::Viewer,
                            })
                            .collect();

                        let display_name = match (&user.first_name, &user.last_name) {
                            (Some(first), Some(last)) => Some(format!("{} {}", first, last)),
                            (Some(first), None) => Some(first.clone()),
                            (None, Some(last)) => Some(last.clone()),
                            (None, None) => None,
                        };

                        return (
                            StatusCode::OK,
                            Json(AuthContext {
                                is_authenticated: true,
                                user: Some(AuthUser {
                                    id: user.id.to_string(),
                                    email: user.email,
                                    display_name,
                                }),
                                roles,
                                auth_mode,
                            }),
                        )
                            .into_response();
                    }
                    _ => {
                        // User not found - treat as unauthenticated
                    }
                }
            }
            _ => {
                // Session not found or invalid - treat as unauthenticated
            }
        }
    }

    // Not authenticated
    (
        StatusCode::OK,
        Json(AuthContext {
            is_authenticated: false,
            user: None,
            roles: vec![],
            auth_mode,
        }),
    )
        .into_response()
}

/// Detect the auth mode from environment variable.
fn detect_auth_mode() -> AuthMode {
    match std::env::var("AUTH_MODE").as_deref() {
        Ok("dev") => AuthMode::Dev,
        Ok("local") => AuthMode::Local,
        Ok("oidc") | Ok("") | Err(_) => AuthMode::Oidc, // Default to OIDC
        _ => AuthMode::Oidc,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    #[test]
    #[serial]
    fn detect_auth_mode_defaults_to_oidc() {
        unsafe {
            std::env::remove_var("AUTH_MODE");
        }
        assert_eq!(detect_auth_mode(), AuthMode::Oidc);
    }

    #[test]
    #[serial]
    fn detect_auth_mode_recognizes_dev() {
        unsafe {
            std::env::set_var("AUTH_MODE", "dev");
        }
        assert_eq!(detect_auth_mode(), AuthMode::Dev);
        unsafe {
            std::env::remove_var("AUTH_MODE");
        }
    }

    #[test]
    #[serial]
    fn detect_auth_mode_recognizes_local() {
        unsafe {
            std::env::set_var("AUTH_MODE", "local");
        }
        assert_eq!(detect_auth_mode(), AuthMode::Local);
        unsafe {
            std::env::remove_var("AUTH_MODE");
        }
    }
}
