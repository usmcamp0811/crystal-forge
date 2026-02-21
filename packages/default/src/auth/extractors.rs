//! Authorization extractors for Axum handlers.
//!
//! This module provides type-safe authorization enforcement through Axum's extractor pattern.
//! Extractors validate session cookies, load user roles, and enforce permission requirements.

use axum::{
    async_trait,
    extract::{FromRef, FromRequestParts},
    http::{request::Parts, StatusCode},
};
use sqlx::PgPool;
use uuid::Uuid;

use crate::auth::session::{extract_cookie, hash_token, SESSION_COOKIE_NAME};
use crate::models::auth_identity::AuthRole;
use crate::queries::auth_identity::{find_active_session_by_token_hash, find_user_roles};

/// Authenticated user context extracted from session cookie.
///
/// Contains the user's ID and all assigned roles. Use this directly in handlers
/// when you need to check roles conditionally, or use one of the role guard
/// extractors (`RequireAuth`, `RequireOperator`, `RequireAdmin`) for declarative
/// permission enforcement.
#[derive(Debug, Clone)]
pub struct AuthenticatedUser {
    pub user_id: Uuid,
    pub roles: Vec<AuthRole>,
}

impl AuthenticatedUser {
    /// Check if the user has a specific role.
    pub fn has_role(&self, role: AuthRole) -> bool {
        self.roles.contains(&role)
    }

    /// Check if the user has the Admin role.
    pub fn is_admin(&self) -> bool {
        self.has_role(AuthRole::Admin)
    }

    /// Check if the user has Admin or Operator role.
    pub fn is_operator_or_higher(&self) -> bool {
        self.has_role(AuthRole::Admin) || self.has_role(AuthRole::Operator)
    }

    /// Check if the user has any role (authenticated).
    pub fn is_authenticated(&self) -> bool {
        !self.roles.is_empty()
    }
}

#[async_trait]
impl<S> FromRequestParts<S> for AuthenticatedUser
where
    S: Send + Sync,
    PgPool: FromRef<S>,
{
    type Rejection = StatusCode;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        // Extract session cookie
        let session_token = extract_cookie(&parts.headers, SESSION_COOKIE_NAME)
            .ok_or(StatusCode::UNAUTHORIZED)?;

        // Hash the token
        let session_hash = hash_token(&session_token);

        // Get database pool from state
        let pool = PgPool::from_ref(state);

        // Look up active session
        let session = find_active_session_by_token_hash(&pool, &session_hash)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .ok_or(StatusCode::UNAUTHORIZED)?;

        // Fetch user roles
        let role_assignments = find_user_roles(&pool, session.user_id)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        let roles = role_assignments.into_iter().map(|ra| ra.role).collect();

        Ok(AuthenticatedUser {
            user_id: session.user_id,
            roles,
        })
    }
}

/// Extractor that requires any authenticated user (any role).
///
/// Returns 401 if no valid session exists.
///
/// # Example
/// ```ignore
/// pub async fn list_items(
///     RequireAuth(user): RequireAuth,
///     State(pool): State<PgPool>,
/// ) -> impl IntoResponse {
///     // user.user_id and user.roles are available
/// }
/// ```
pub struct RequireAuth(pub AuthenticatedUser);

#[async_trait]
impl<S> FromRequestParts<S> for RequireAuth
where
    S: Send + Sync,
    PgPool: FromRef<S>,
{
    type Rejection = StatusCode;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let user = AuthenticatedUser::from_request_parts(parts, state).await?;
        Ok(RequireAuth(user))
    }
}

/// Extractor that requires Admin or Operator role.
///
/// Returns 401 if no valid session exists.
/// Returns 403 if the user is authenticated but lacks Operator or Admin role.
///
/// # Example
/// ```ignore
/// pub async fn create_flake(
///     RequireOperator(user): RequireOperator,
///     State(pool): State<PgPool>,
///     Json(payload): Json<CreateFlakeRequest>,
/// ) -> impl IntoResponse {
///     // Only Admins and Operators reach here
/// }
/// ```
pub struct RequireOperator(pub AuthenticatedUser);

#[async_trait]
impl<S> FromRequestParts<S> for RequireOperator
where
    S: Send + Sync,
    PgPool: FromRef<S>,
{
    type Rejection = StatusCode;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let user = AuthenticatedUser::from_request_parts(parts, state).await?;

        if !user.is_operator_or_higher() {
            return Err(StatusCode::FORBIDDEN);
        }

        Ok(RequireOperator(user))
    }
}

/// Extractor that requires Admin role.
///
/// Returns 401 if no valid session exists.
/// Returns 403 if the user is authenticated but lacks Admin role.
///
/// # Example
/// ```ignore
/// pub async fn delete_flake(
///     RequireAdmin(user): RequireAdmin,
///     State(pool): State<PgPool>,
///     Path(flake_id): Path<i32>,
/// ) -> impl IntoResponse {
///     // Only Admins reach here
/// }
/// ```
pub struct RequireAdmin(pub AuthenticatedUser);

#[async_trait]
impl<S> FromRequestParts<S> for RequireAdmin
where
    S: Send + Sync,
    PgPool: FromRef<S>,
{
    type Rejection = StatusCode;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let user = AuthenticatedUser::from_request_parts(parts, state).await?;

        if !user.is_admin() {
            return Err(StatusCode::FORBIDDEN);
        }

        Ok(RequireAdmin(user))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_role_checks() {
        let admin_user = AuthenticatedUser {
            user_id: Uuid::new_v4(),
            roles: vec![AuthRole::Admin],
        };

        assert!(admin_user.is_admin());
        assert!(admin_user.is_operator_or_higher());
        assert!(admin_user.has_role(AuthRole::Admin));
        assert!(!admin_user.has_role(AuthRole::Viewer));

        let operator_user = AuthenticatedUser {
            user_id: Uuid::new_v4(),
            roles: vec![AuthRole::Operator],
        };

        assert!(!operator_user.is_admin());
        assert!(operator_user.is_operator_or_higher());
        assert!(operator_user.has_role(AuthRole::Operator));

        let viewer_user = AuthenticatedUser {
            user_id: Uuid::new_v4(),
            roles: vec![AuthRole::Viewer],
        };

        assert!(!viewer_user.is_admin());
        assert!(!viewer_user.is_operator_or_higher());
        assert!(viewer_user.has_role(AuthRole::Viewer));
        assert!(viewer_user.is_authenticated());
    }

    #[test]
    fn test_multiple_roles() {
        let multi_role_user = AuthenticatedUser {
            user_id: Uuid::new_v4(),
            roles: vec![AuthRole::Admin, AuthRole::Operator],
        };

        assert!(multi_role_user.is_admin());
        assert!(multi_role_user.is_operator_or_higher());
        assert!(multi_role_user.has_role(AuthRole::Admin));
        assert!(multi_role_user.has_role(AuthRole::Operator));
    }
}
