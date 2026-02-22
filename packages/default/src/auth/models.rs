use crate::models::auth_identity::AuthRole;
use std::collections::BTreeSet;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Admin,
    Operator,
    Viewer,
}

impl From<Role> for AuthRole {
    fn from(value: Role) -> Self {
        match value {
            Role::Admin => AuthRole::Admin,
            Role::Operator => AuthRole::Operator,
            Role::Viewer => AuthRole::Viewer,
        }
    }
}

impl From<AuthRole> for Role {
    fn from(value: AuthRole) -> Self {
        match value {
            AuthRole::Admin => Role::Admin,
            AuthRole::Operator => Role::Operator,
            AuthRole::Viewer => Role::Viewer,
        }
    }
}

impl Role {
    pub fn can_view_systems(self) -> bool {
        true
    }

    pub fn can_mutate_systems(self) -> bool {
        matches!(self, Role::Admin | Role::Operator)
    }

    pub fn can_manage_environments(self) -> bool {
        matches!(self, Role::Admin)
    }

    pub fn can_manage_admin_console(self) -> bool {
        matches!(self, Role::Admin)
    }

    pub fn can_access_system_environment(
        self,
        system_environment_id: Option<Uuid>,
        member_environment_ids: &BTreeSet<Uuid>,
    ) -> bool {
        if matches!(self, Role::Admin) {
            return true;
        }

        match system_environment_id {
            Some(environment_id) => member_environment_ids.contains(&environment_id),
            None => member_environment_ids.is_empty(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Role, *};

    #[test]
    fn auth_models_role_converts_to_db_role() {
        assert_eq!(AuthRole::from(Role::Admin), AuthRole::Admin);
        assert_eq!(AuthRole::from(Role::Operator), AuthRole::Operator);
        assert_eq!(AuthRole::from(Role::Viewer), AuthRole::Viewer);
    }

    #[test]
    fn role_policy_matrix_matches_rbac_expectations() {
        assert!(Role::Viewer.can_view_systems());
        assert!(!Role::Viewer.can_mutate_systems());
        assert!(!Role::Viewer.can_manage_environments());
        assert!(!Role::Viewer.can_manage_admin_console());

        assert!(Role::Operator.can_view_systems());
        assert!(Role::Operator.can_mutate_systems());
        assert!(!Role::Operator.can_manage_environments());
        assert!(!Role::Operator.can_manage_admin_console());

        assert!(Role::Admin.can_view_systems());
        assert!(Role::Admin.can_mutate_systems());
        assert!(Role::Admin.can_manage_environments());
        assert!(Role::Admin.can_manage_admin_console());
    }

    #[test]
    fn environment_scope_access_requires_membership_for_non_admins() {
        let staging_id = Uuid::parse_str("00000000-0000-0000-0000-000000000010").expect("uuid");
        let prod_id = Uuid::parse_str("00000000-0000-0000-0000-000000000011").expect("uuid");

        let mut staging_only = BTreeSet::new();
        staging_only.insert(staging_id);

        assert!(Role::Admin.can_access_system_environment(Some(prod_id), &BTreeSet::new()));

        assert!(Role::Operator.can_access_system_environment(Some(staging_id), &staging_only));
        assert!(!Role::Operator.can_access_system_environment(Some(prod_id), &staging_only));
        assert!(Role::Viewer.can_access_system_environment(Some(staging_id), &staging_only));
        assert!(!Role::Viewer.can_access_system_environment(None, &staging_only));

        assert!(Role::Viewer.can_access_system_environment(None, &BTreeSet::new()));
    }
}
