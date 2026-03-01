//! Development-only authentication mode with fixed role selector.
//!
//! This module provides a development-only auth bypass that creates fixture users
//! for Admin, Operator, and Viewer roles without requiring external OIDC setup.

use crate::auth::models::Role;
use crate::models::auth_identity::{AuthRole, UserRoleAssignment};
use crate::models::users::User;
use crate::queries::auth_identity::{assign_role_to_user, find_user_roles};
use crate::queries::users::{get_by_email, get_user_by_email, insert_user};
use anyhow::{Context, Result};
use sqlx::PgPool;
use uuid::Uuid;

const DEV_ADMIN_EMAIL: &str = "dev-admin@crystal-forge.local";
const DEV_OPERATOR_EMAIL: &str = "dev-operator@crystal-forge.local";
const DEV_VIEWER_EMAIL: &str = "dev-viewer@crystal-forge.local";

/// Development mode user fixture definition.
#[derive(Debug, Clone)]
pub struct DevUser {
    pub email: String,
    pub display_name: String,
    pub role: Role,
}

/// Get all development mode user fixtures.
pub fn dev_user_fixtures() -> Vec<DevUser> {
    vec![
        DevUser {
            email: DEV_ADMIN_EMAIL.to_string(),
            display_name: "Dev Admin".to_string(),
            role: Role::Admin,
        },
        DevUser {
            email: DEV_OPERATOR_EMAIL.to_string(),
            display_name: "Dev Operator".to_string(),
            role: Role::Operator,
        },
        DevUser {
            email: DEV_VIEWER_EMAIL.to_string(),
            display_name: "Dev Viewer".to_string(),
            role: Role::Viewer,
        },
    ]
}

/// Ensure all development mode fixture users exist in the database with appropriate roles.
///
/// This is idempotent and safe to call on startup in dev mode.
pub async fn ensure_dev_users(pool: &PgPool) -> Result<()> {
    for fixture in dev_user_fixtures() {
        // Use get_by_email which returns Option<User> to distinguish "not found" from DB errors
        let user = match get_by_email(pool, &fixture.email).await? {
            Some(user) => user,
            None => insert_user(pool, &fixture.email, Some(&fixture.display_name))
                .await
                .context("Failed to insert dev fixture user")?,
        };

        let existing_roles = find_user_roles(pool, user.id).await?;
        let has_role = existing_roles.iter().any(|r| r.role == fixture.role.into());

        if !has_role {
            assign_role_to_user(pool, user.id, fixture.role.into(), None)
                .await
                .context("Failed to assign role to dev fixture user")?;
        }
    }

    Ok(())
}

/// Find a dev fixture user by email.
pub async fn find_dev_user_by_email(pool: &PgPool, email: &str) -> Result<User> {
    get_user_by_email(pool, email)
        .await
        .context("Dev fixture user not found")
}

/// Validate that the given email is a valid dev mode fixture email.
pub fn is_valid_dev_user_email(email: &str) -> bool {
    matches!(
        email,
        DEV_ADMIN_EMAIL | DEV_OPERATOR_EMAIL | DEV_VIEWER_EMAIL
    )
}

/// Bootstrap OIDC group mapping for initial admin access.
///
/// If CRYSTAL_FORGE_OIDC_BOOTSTRAP_ADMIN_GROUP is set, this creates a mapping
/// from that OIDC group to the Admin role. This allows you to grant initial admin
/// access when first deploying with OIDC authentication.
///
/// The bootstrap mapping is created only if:
/// 1. The environment variable is set
/// 2. No mapping for that group name already exists
///
/// This is idempotent and safe to call on every startup. After the first admin
/// logs in, they can configure additional mappings via the admin UI.
///
/// Example usage:
/// ```bash
/// # For production with Entra ID:
/// CRYSTAL_FORGE_OIDC_BOOTSTRAP_ADMIN_GROUP=platform-admins
///
/// # For development with Keycloak:
/// CRYSTAL_FORGE_OIDC_BOOTSTRAP_ADMIN_GROUP=admin
/// ```
pub async fn ensure_bootstrap_oidc_admin_mapping(pool: &PgPool) -> Result<()> {
    use crate::config::OidcConfig;
    use crate::queries::auth_identity::{count_oidc_group_mappings, create_oidc_group_mapping};

    let Some(admin_group) = OidcConfig::bootstrap_admin_group() else {
        // No bootstrap group configured, skip
        return Ok(());
    };

    tracing::info!("Checking for OIDC bootstrap admin mapping: {}", admin_group);

    // Check if mapping already exists
    let existing = count_oidc_group_mappings(pool, &admin_group)
        .await
        .context("Failed to check existing OIDC group mapping")?;

    if existing > 0 {
        tracing::debug!(
            "Bootstrap admin mapping already exists for: {}",
            admin_group
        );
        return Ok(());
    }

    // Create the bootstrap admin mapping (with no environment restrictions)
    create_oidc_group_mapping(pool, &admin_group, AuthRole::Admin, &[])
        .await
        .context(format!(
            "Failed to create bootstrap OIDC admin mapping for {}",
            admin_group
        ))?;

    tracing::info!(
        "✅ Created bootstrap OIDC admin mapping: {} → Admin",
        admin_group
    );
    tracing::info!(
        "   Users in the '{}' OIDC group will now have Admin access",
        admin_group
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dev_user_fixtures_returns_three_roles() {
        let fixtures = dev_user_fixtures();
        assert_eq!(fixtures.len(), 3);

        let roles: Vec<Role> = fixtures.iter().map(|f| f.role).collect();
        assert!(roles.contains(&Role::Admin));
        assert!(roles.contains(&Role::Operator));
        assert!(roles.contains(&Role::Viewer));
    }

    #[test]
    fn is_valid_dev_user_email_accepts_fixtures() {
        assert!(is_valid_dev_user_email(DEV_ADMIN_EMAIL));
        assert!(is_valid_dev_user_email(DEV_OPERATOR_EMAIL));
        assert!(is_valid_dev_user_email(DEV_VIEWER_EMAIL));
    }

    #[test]
    fn is_valid_dev_user_email_rejects_unknown() {
        assert!(!is_valid_dev_user_email("unknown@example.com"));
        assert!(!is_valid_dev_user_email(""));
    }
}
