//! Authentication state helpers and role checking utilities.

use crate::api::models::{AuthContext, Role};

/// Get the effective role for the current user, considering masquerade state.
///
/// If masquerade_role is Some and the real user is Admin, returns the masquerade role.
/// Otherwise returns the real user's primary role.
pub fn get_effective_role(
    auth: &Option<AuthContext>,
    masquerade_role: &Option<Role>,
) -> Option<Role> {
    // If masquerading and real user is Admin, use masquerade role
    if let Some(masq_role) = masquerade_role {
        if is_admin_real(auth) {
            return Some(*masq_role);
        }
    }

    // Otherwise use real role
    auth.as_ref()
        .filter(|ctx| ctx.is_authenticated)
        .and_then(|ctx| ctx.roles.first().copied())
}

/// Check if the real (not masqueraded) user has Admin role.
/// This is used for authorization checks that should never be affected by masquerade.
fn is_admin_real(auth: &Option<AuthContext>) -> bool {
    match auth {
        Some(ctx) if ctx.is_authenticated => ctx.roles.contains(&Role::Admin),
        _ => false,
    }
}

/// Check if the current user has at least one of the specified roles (respects masquerade).
pub fn has_any_role(
    auth: &Option<AuthContext>,
    masquerade_role: &Option<Role>,
    required_roles: &[Role],
) -> bool {
    if let Some(effective_role) = get_effective_role(auth, masquerade_role) {
        required_roles.contains(&effective_role)
    } else {
        false
    }
}

/// Check if the current user has the Admin role (respects masquerade).
pub fn is_admin(auth: &Option<AuthContext>, masquerade_role: &Option<Role>) -> bool {
    has_any_role(auth, masquerade_role, &[Role::Admin])
}

/// Check if the current user has the Operator role or higher (respects masquerade).
pub fn is_operator_or_above(auth: &Option<AuthContext>, masquerade_role: &Option<Role>) -> bool {
    has_any_role(auth, masquerade_role, &[Role::Admin, Role::Operator])
}

/// Check if the current user can perform mutating system actions (respects masquerade).
pub fn can_mutate_systems(auth: &Option<AuthContext>, masquerade_role: &Option<Role>) -> bool {
    has_any_role(auth, masquerade_role, &[Role::Admin, Role::Operator])
}

/// Check if the current user can manage environments (respects masquerade).
pub fn can_manage_environments(auth: &Option<AuthContext>, masquerade_role: &Option<Role>) -> bool {
    has_any_role(auth, masquerade_role, &[Role::Admin])
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
        assert!(is_admin(&auth_context(true, vec![Role::Admin]), &None));
        assert!(!is_admin(&auth_context(true, vec![Role::Operator]), &None));
        assert!(!is_admin(&auth_context(false, vec![Role::Admin]), &None));
        assert!(!is_admin(&None, &None));
    }

    #[test]
    fn is_operator_or_above_accepts_operator_and_admin() {
        assert!(is_operator_or_above(
            &auth_context(true, vec![Role::Operator]),
            &None
        ));
        assert!(is_operator_or_above(
            &auth_context(true, vec![Role::Admin]),
            &None
        ));
        assert!(!is_operator_or_above(
            &auth_context(true, vec![Role::Viewer]),
            &None
        ));
    }

    #[test]
    fn can_mutate_systems_requires_operator_or_admin() {
        assert!(can_mutate_systems(
            &auth_context(true, vec![Role::Admin]),
            &None
        ));
        assert!(can_mutate_systems(
            &auth_context(true, vec![Role::Operator]),
            &None
        ));
        assert!(!can_mutate_systems(
            &auth_context(true, vec![Role::Viewer]),
            &None
        ));
        assert!(!can_mutate_systems(
            &auth_context(false, vec![Role::Operator]),
            &None
        ));
    }

    #[test]
    fn can_manage_environments_requires_admin() {
        assert!(can_manage_environments(
            &auth_context(true, vec![Role::Admin]),
            &None
        ));
        assert!(!can_manage_environments(
            &auth_context(true, vec![Role::Operator]),
            &None
        ));
        assert!(!can_manage_environments(
            &auth_context(true, vec![Role::Viewer]),
            &None
        ));
    }

    #[test]
    fn masquerade_as_operator_hides_admin_privileges() {
        let admin_ctx = auth_context(true, vec![Role::Admin]);
        let masq = Some(Role::Operator);

        // Real admin can manage environments
        assert!(can_manage_environments(&admin_ctx, &None));
        // But when masquerading as Operator, cannot
        assert!(!can_manage_environments(&admin_ctx, &masq));
        // Can still mutate systems as Operator
        assert!(can_mutate_systems(&admin_ctx, &masq));
    }

    #[test]
    fn masquerade_as_viewer_hides_all_mutate_privileges() {
        let admin_ctx = auth_context(true, vec![Role::Admin]);
        let masq = Some(Role::Viewer);

        // Real admin can mutate
        assert!(can_mutate_systems(&admin_ctx, &None));
        // But when masquerading as Viewer, cannot
        assert!(!can_mutate_systems(&admin_ctx, &masq));
    }

    #[test]
    fn non_admin_cannot_masquerade() {
        let operator_ctx = auth_context(true, vec![Role::Operator]);
        let masq = Some(Role::Admin);

        // Operator trying to masquerade as Admin should not gain privileges
        assert!(!can_manage_environments(&operator_ctx, &masq));
    }
}
