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
