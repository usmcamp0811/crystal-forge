use axum::http::HeaderMap;
use sqlx::PgPool;
use uuid::Uuid;

use crate::auth::session::{SESSION_COOKIE_NAME, extract_cookie, hash_token};
use crate::models::auth_identity::AuthRole;
use crate::queries::auth_identity::{get_session_by_token_hash, get_user_roles};

pub async fn require_admin(pool: &PgPool, headers: &HeaderMap) -> Option<Uuid> {
    let (user_id, roles) = resolve_authenticated_roles(pool, headers).await?;
    if has_admin_role(&roles) {
        Some(user_id)
    } else {
        None
    }
}

pub async fn require_operator_or_admin(pool: &PgPool, headers: &HeaderMap) -> Option<Uuid> {
    let (user_id, roles) = resolve_authenticated_roles(pool, headers).await?;
    if has_operator_or_admin_role(&roles) {
        Some(user_id)
    } else {
        None
    }
}

pub async fn require_viewer_or_above(pool: &PgPool, headers: &HeaderMap) -> Option<Uuid> {
    let (user_id, roles) = resolve_authenticated_roles(pool, headers).await?;
    if has_viewer_or_above_role(&roles) {
        Some(user_id)
    } else {
        None
    }
}

pub async fn authenticated_user_roles(
    pool: &PgPool,
    headers: &HeaderMap,
) -> Option<(Uuid, Vec<AuthRole>)> {
    resolve_authenticated_roles(pool, headers).await
}

pub fn has_admin_role(roles: &[AuthRole]) -> bool {
    roles.contains(&AuthRole::Admin)
}

pub fn has_operator_or_admin_role(roles: &[AuthRole]) -> bool {
    roles
        .iter()
        .any(|role| matches!(role, AuthRole::Admin | AuthRole::Operator))
}

pub fn has_viewer_or_above_role(roles: &[AuthRole]) -> bool {
    roles.iter().any(|role| {
        matches!(
            role,
            AuthRole::Admin | AuthRole::Operator | AuthRole::Viewer
        )
    })
}

async fn resolve_authenticated_roles(
    pool: &PgPool,
    headers: &HeaderMap,
) -> Option<(Uuid, Vec<AuthRole>)> {
    let token = extract_cookie(headers, SESSION_COOKIE_NAME)?;
    let token_hash = hash_token(&token);
    let session = get_session_by_token_hash(pool, &token_hash).await.ok()??;

    if session.is_expired() || session.is_invalidated() {
        return None;
    }

    let roles = get_user_roles(pool, session.user_id)
        .await
        .ok()?
        .into_iter()
        .map(|assignment| assignment.role)
        .collect::<Vec<_>>();

    Some((session.user_id, roles))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn role_predicates_match_expected_matrix() {
        assert!(has_admin_role(&[AuthRole::Admin]));
        assert!(!has_admin_role(&[AuthRole::Operator]));
        assert!(!has_admin_role(&[AuthRole::Viewer]));

        assert!(has_operator_or_admin_role(&[AuthRole::Admin]));
        assert!(has_operator_or_admin_role(&[AuthRole::Operator]));
        assert!(has_operator_or_admin_role(&[
            AuthRole::Viewer,
            AuthRole::Operator,
        ]));
        assert!(!has_operator_or_admin_role(&[AuthRole::Viewer]));

        assert!(has_viewer_or_above_role(&[AuthRole::Viewer]));
        assert!(has_viewer_or_above_role(&[AuthRole::Operator]));
        assert!(has_viewer_or_above_role(&[AuthRole::Admin]));
        assert!(!has_viewer_or_above_role(&[]));
    }
}
