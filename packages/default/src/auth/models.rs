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

#[cfg(test)]
mod tests {
    use super::{Role, *};

    #[test]
    fn auth_models_role_converts_to_db_role() {
        assert_eq!(AuthRole::from(Role::Admin), AuthRole::Admin);
        assert_eq!(AuthRole::from(Role::Operator), AuthRole::Operator);
        assert_eq!(AuthRole::from(Role::Viewer), AuthRole::Viewer);
    }
}
