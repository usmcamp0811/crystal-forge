use crate::models::auth_identity::AuthRole;

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
}
