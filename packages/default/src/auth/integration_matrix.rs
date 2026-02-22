use crate::auth::oidc::ClaimExtractor;
use crate::config::ClaimMappingConfig;
use serde_json::{json, Value};
use std::collections::HashMap;

fn extract_roles_for_provider(roles_claim: &str, roles_value: Value) -> Vec<String> {
    let mut claims = HashMap::new();

    if roles_claim.contains('.') {
        let mut nested = serde_json::Map::new();
        let segments: Vec<&str> = roles_claim.split('.').collect();
        if segments.len() == 2 {
            nested.insert(segments[1].to_string(), roles_value);
            claims.insert(segments[0].to_string(), Value::Object(nested));
        }
    } else {
        claims.insert(roles_claim.to_string(), roles_value);
    }

    let config = ClaimMappingConfig {
        roles_claim: roles_claim.to_string(),
        ..ClaimMappingConfig::default()
    };

    let extractor = ClaimExtractor::new(config);
    extractor
        .extract_user_info(
            None,
            None,
            None,
            None,
            None,
            None,
            &claims,
            "subject-1".to_string(),
        )
        .unwrap()
        .roles
}

#[test]
fn provider_matrix_authentik_groups_claim() {
    let roles = extract_roles_for_provider("groups", json!(["admin", "operator"]));
    assert_eq!(roles, vec!["admin", "operator"]);
}

#[test]
fn provider_matrix_keycloak_realm_access_roles_claim() {
    let roles = extract_roles_for_provider("realm_access.roles", json!(["admin", "viewer"]));
    assert_eq!(roles, vec!["admin", "viewer"]);
}

#[test]
fn provider_matrix_entra_roles_claim() {
    let roles = extract_roles_for_provider("roles", json!(["Reader", "Writer"]));
    assert_eq!(roles, vec!["Reader", "Writer"]);
}

#[test]
fn provider_matrix_okta_groups_claim() {
    let roles = extract_roles_for_provider("groups", json!(["cf-admin", "cf-operator"]));
    assert_eq!(roles, vec!["cf-admin", "cf-operator"]);
}

#[test]
fn provider_matrix_generic_oidc_comma_separated_roles() {
    let roles = extract_roles_for_provider("roles", json!("admin,operator,viewer"));
    assert_eq!(roles, vec!["admin", "operator", "viewer"]);
}
