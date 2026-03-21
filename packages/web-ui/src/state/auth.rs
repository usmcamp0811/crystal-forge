//! Authentication state helpers and role checking utilities.
//!
//! This module separates three concerns:
//! 1. Authorization (real role only - never affected by masquerade)
//! 2. UI Display (masquerade-aware - for rendering preview)
//! 3. User info (display names, etc.)

use crate::api::models::{AuthContext, Role};

// ============================================================================
// AUTHORIZATION FUNCTIONS (Real Role Only - Never Masquerade)
// ============================================================================

/// Check if user has any of the required roles (multi-role safe).
/// This is for AUTHORIZATION and ignores masquerade state.
fn has_any_real_role(auth: &Option<AuthContext>, required_roles: &[Role]) -> bool {
    match auth {
        Some(ctx) if ctx.is_authenticated => {
            required_roles.iter().any(|role| ctx.roles.contains(role))
        }
        _ => false,
    }
}

/// Check if the current user has the Admin role.
/// Used for AUTHORIZATION - never affected by masquerade.
pub fn is_admin(auth: &Option<AuthContext>) -> bool {
    has_any_real_role(auth, &[Role::Admin])
}

/// Check if the current user has the Operator role or higher.
/// Used for AUTHORIZATION - never affected by masquerade.
pub fn is_operator_or_above(auth: &Option<AuthContext>) -> bool {
    has_any_real_role(auth, &[Role::Admin, Role::Operator])
}

/// Check if the current user can perform mutating system actions.
/// Used for AUTHORIZATION - never affected by masquerade.
pub fn can_mutate_systems(auth: &Option<AuthContext>) -> bool {
    has_any_real_role(auth, &[Role::Admin, Role::Operator])
}

/// Check if the current user can manage environments.
/// Used for AUTHORIZATION - never affected by masquerade.
pub fn can_manage_environments(auth: &Option<AuthContext>) -> bool {
    has_any_real_role(auth, &[Role::Admin])
}

// ============================================================================
// UI DISPLAY FUNCTIONS (Masquerade-Aware - For Rendering)
// ============================================================================

/// Get the highest privilege role from user's real roles.
/// Used for badge display and determining real capabilities.
/// Returns Admin > Operator > Viewer in priority order.
pub fn get_highest_real_role(auth: &Option<AuthContext>) -> Option<Role> {
    auth.as_ref()
        .filter(|ctx| ctx.is_authenticated)
        .and_then(|ctx| {
            // Check in priority order: Admin > Operator > Viewer
            if ctx.roles.contains(&Role::Admin) {
                Some(Role::Admin)
            } else if ctx.roles.contains(&Role::Operator) {
                Some(Role::Operator)
            } else if ctx.roles.contains(&Role::Viewer) {
                Some(Role::Viewer)
            } else {
                None
            }
        })
}

/// Get the display role for UI rendering (respects masquerade).
/// When masquerading, returns the masquerade role.
/// When not masquerading, returns the highest privilege real role.
/// This is used ONLY for UI display, never for authorization.
pub fn get_display_role(
    auth: &Option<AuthContext>,
    masquerade_role: &Option<Role>,
) -> Option<Role> {
    // If masquerading and user is admin, use masquerade role
    if let Some(masq_role) = masquerade_role {
        if is_admin(auth) {
            return Some(*masq_role);
        }
    }

    // Otherwise use highest privilege real role
    get_highest_real_role(auth)
}

/// Check if a UI element should be shown based on display role.
/// This respects masquerade for UI preview purposes only.
/// Use this for conditional rendering of buttons, sections, etc.
pub fn should_show_for_display_role(
    auth: &Option<AuthContext>,
    masquerade_role: &Option<Role>,
    required_roles: &[Role],
) -> bool {
    if let Some(display_role) = get_display_role(auth, masquerade_role) {
        required_roles.contains(&display_role)
    } else {
        false
    }
}

// ============================================================================
// USER INFO FUNCTIONS
// ============================================================================

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

    // ========================================================================
    // AUTHORIZATION TESTS (Real Role Only - Ignore Masquerade)
    // ========================================================================

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

    #[test]
    fn authorization_ignores_masquerade() {
        let admin_ctx = auth_context(true, vec![Role::Admin]);

        // Authorization functions don't take masquerade parameter
        // Admin always has admin permissions regardless of UI state
        assert!(is_admin(&admin_ctx));
        assert!(can_mutate_systems(&admin_ctx));
        assert!(can_manage_environments(&admin_ctx));
    }

    #[test]
    fn multi_role_authorization_checks_all_roles() {
        let ctx = auth_context(true, vec![Role::Viewer, Role::Admin]);

        // Should have admin privileges regardless of role order
        assert!(is_admin(&ctx));
        assert!(can_manage_environments(&ctx));
    }

    #[test]
    fn multi_role_viewer_admin_grants_admin_privileges() {
        let ctx = auth_context(true, vec![Role::Viewer, Role::Admin]);

        // User with [Viewer, Admin] should have Admin privileges
        assert!(is_admin(&ctx));
        assert!(can_manage_environments(&ctx));
        assert!(can_mutate_systems(&ctx));
    }

    #[test]
    fn multi_role_operator_admin_grants_admin_privileges() {
        let ctx = auth_context(true, vec![Role::Operator, Role::Admin]);

        // User with [Operator, Admin] should have Admin privileges
        assert!(is_admin(&ctx));
        assert!(can_manage_environments(&ctx));
        assert!(is_operator_or_above(&ctx));
    }

    #[test]
    fn multi_role_admin_operator_grants_admin_privileges_reversed_order() {
        let ctx = auth_context(true, vec![Role::Admin, Role::Operator]);

        // User with [Admin, Operator] (reversed) should still have Admin privileges
        assert!(is_admin(&ctx));
        assert!(can_manage_environments(&ctx));
        assert!(is_operator_or_above(&ctx));
    }

    #[test]
    fn multi_role_admin_viewer_reversed_order() {
        let ctx = auth_context(true, vec![Role::Admin, Role::Viewer]);

        // User with [Admin, Viewer] should have Admin privileges
        assert!(is_admin(&ctx));
        assert!(can_manage_environments(&ctx));
    }

    // ========================================================================
    // DISPLAY ROLE TESTS (UI Preview - Masquerade Aware)
    // ========================================================================

    #[test]
    fn get_highest_real_role_returns_admin_for_multi_role() {
        assert_eq!(
            get_highest_real_role(&auth_context(true, vec![Role::Viewer, Role::Admin])),
            Some(Role::Admin)
        );

        assert_eq!(
            get_highest_real_role(&auth_context(true, vec![Role::Admin, Role::Viewer])),
            Some(Role::Admin)
        );
    }

    #[test]
    fn get_highest_real_role_hierarchy() {
        assert_eq!(
            get_highest_real_role(&auth_context(
                true,
                vec![Role::Viewer, Role::Operator, Role::Admin]
            )),
            Some(Role::Admin)
        );

        assert_eq!(
            get_highest_real_role(&auth_context(true, vec![Role::Viewer, Role::Operator])),
            Some(Role::Operator)
        );

        assert_eq!(
            get_highest_real_role(&auth_context(true, vec![Role::Viewer])),
            Some(Role::Viewer)
        );
    }

    #[test]
    fn display_role_respects_masquerade() {
        let admin_ctx = auth_context(true, vec![Role::Admin]);
        let masq = Some(Role::Viewer);

        // Display role should be Viewer when masquerading
        assert_eq!(get_display_role(&admin_ctx, &masq), Some(Role::Viewer));

        // Without masquerade, display role is real role
        assert_eq!(get_display_role(&admin_ctx, &None), Some(Role::Admin));
    }

    #[test]
    fn display_role_shows_highest_privilege_for_multi_role() {
        let ctx = auth_context(true, vec![Role::Viewer, Role::Admin]);

        // Should display Admin (highest privilege) not Viewer
        assert_eq!(get_display_role(&ctx, &None), Some(Role::Admin));
    }

    #[test]
    fn should_show_for_display_role_respects_masquerade() {
        let admin_ctx = auth_context(true, vec![Role::Admin]);
        let masq_viewer = Some(Role::Viewer);

        // When masquerading as Viewer, admin UI elements should be hidden
        assert!(!should_show_for_display_role(
            &admin_ctx,
            &masq_viewer,
            &[Role::Admin]
        ));

        // But viewer UI elements should be shown
        assert!(should_show_for_display_role(
            &admin_ctx,
            &masq_viewer,
            &[Role::Viewer]
        ));

        // Without masquerade, admin UI elements should be shown
        assert!(should_show_for_display_role(
            &admin_ctx,
            &None,
            &[Role::Admin]
        ));
    }

    #[test]
    fn masquerade_as_operator_hides_admin_ui_but_not_operator_ui() {
        let admin_ctx = auth_context(true, vec![Role::Admin]);
        let masq = Some(Role::Operator);

        // Admin UI elements hidden when masquerading as Operator
        assert!(!should_show_for_display_role(
            &admin_ctx,
            &masq,
            &[Role::Admin]
        ));

        // Operator UI elements shown
        assert!(should_show_for_display_role(
            &admin_ctx,
            &masq,
            &[Role::Operator]
        ));

        // Multi-role check: Admin or Operator
        assert!(should_show_for_display_role(
            &admin_ctx,
            &masq,
            &[Role::Admin, Role::Operator]
        ));
    }

    #[test]
    fn non_admin_cannot_masquerade() {
        let operator_ctx = auth_context(true, vec![Role::Operator]);
        let masq = Some(Role::Admin);

        // Operator trying to masquerade as Admin should not affect display
        // (masquerade only works for admins)
        assert_eq!(get_display_role(&operator_ctx, &masq), Some(Role::Operator));
    }
}
