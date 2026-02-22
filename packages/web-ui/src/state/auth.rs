//! Authentication state helpers and role checking utilities.

use crate::api::models::{AuthContext, Role};

/// Check if the current user has at least one of the specified roles.
pub fn has_any_role(auth: &Option<AuthContext>, required_roles: &[Role]) -> bool {
    match auth {
        Some(ctx) if ctx.is_authenticated => {
            required_roles.iter().any(|role| ctx.roles.contains(role))
        }
        _ => false,
    }
}

/// Check if the current user has the Admin role.
pub fn is_admin(auth: &Option<AuthContext>) -> bool {
    has_any_role(auth, &[Role::Admin])
}

/// Check if the current user has the Operator role (or higher).
pub fn is_operator_or_above(auth: &Option<AuthContext>) -> bool {
    has_any_role(auth, &[Role::Admin, Role::Operator])
}

/// Check if the current user can perform mutating system actions.
pub fn can_mutate_systems(auth: &Option<AuthContext>) -> bool {
    has_any_role(auth, &[Role::Admin, Role::Operator])
}

/// Check if the current user can manage environments.
pub fn can_manage_environments(auth: &Option<AuthContext>) -> bool {
    has_any_role(auth, &[Role::Admin])
}

/// Check if the current user is authenticated.
pub fn is_authenticated(auth: &Option<AuthContext>) -> bool {
    auth.as_ref()
        .map(|ctx| ctx.is_authenticated)
        .unwrap_or(false)
}

/// Get the display name or email of the current user.
pub fn user_display_name(auth: &Option<AuthContext>) -> Option<String> {
    auth.as_ref().and_then(|ctx| ctx.user.as_ref()).map(|user| {
        user.display_name
            .clone()
            .unwrap_or_else(|| user.email.clone())
    })
}

/// Get a short display for the user (first name or email prefix).
pub fn user_short_name(auth: &Option<AuthContext>) -> Option<String> {
    auth.as_ref().and_then(|ctx| ctx.user.as_ref()).map(|user| {
        user.display_name
            .as_ref()
            .and_then(|name| name.split_whitespace().next().map(String::from))
            .unwrap_or_else(|| {
                user.email
                    .split('@')
                    .next()
                    .unwrap_or(&user.email)
                    .to_string()
            })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::models::{AuthMode, AuthUser};

    fn auth_context(authenticated: bool, roles: Vec<Role>) -> Option<AuthContext> {
        Some(AuthContext {
            is_authenticated: authenticated,
            user: Some(AuthUser {
                id: "user-1".to_string(),
                email: "user@example.com".to_string(),
                display_name: Some("Example User".to_string()),
            }),
            roles,
            auth_mode: AuthMode::Local,
        })
    }

    #[test]
    fn is_admin_requires_authenticated_admin_role() {
        assert!(is_admin(&auth_context(true, vec![Role::Admin])));
        assert!(!is_admin(&auth_context(true, vec![Role::Operator])));
        assert!(!is_admin(&auth_context(false, vec![Role::Admin])));
        assert!(!is_admin(&None));
    }

    #[test]
    fn is_operator_or_above_accepts_operator_and_admin() {
        assert!(is_operator_or_above(&auth_context(
            true,
            vec![Role::Operator]
        )));
        assert!(is_operator_or_above(&auth_context(true, vec![Role::Admin])));
        assert!(!is_operator_or_above(&auth_context(
            true,
            vec![Role::Viewer]
        )));
    }

    #[test]
    fn can_mutate_systems_requires_operator_or_admin() {
        assert!(can_mutate_systems(&auth_context(true, vec![Role::Admin])));
        assert!(can_mutate_systems(&auth_context(
            true,
            vec![Role::Operator]
        )));
        assert!(!can_mutate_systems(&auth_context(true, vec![Role::Viewer])));
        assert!(!can_mutate_systems(&auth_context(
            false,
            vec![Role::Operator]
        )));
    }

    #[test]
    fn can_manage_environments_requires_admin() {
        assert!(can_manage_environments(&auth_context(
            true,
            vec![Role::Admin]
        )));
        assert!(!can_manage_environments(&auth_context(
            true,
            vec![Role::Operator]
        )));
        assert!(!can_manage_environments(&auth_context(
            true,
            vec![Role::Viewer]
        )));
    }
}
