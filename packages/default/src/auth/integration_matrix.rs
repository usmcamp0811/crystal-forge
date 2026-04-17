use crate::auth::oidc::ClaimExtractor;
use crate::config::ClaimMappingConfig;
use serde_json::{Value, json};
use std::collections::HashMap;

fn extract_roles_for_provider(roles_claim: &str, roles_value: Value) -> Vec<String> {
    let mut claims: HashMap<String, Value> = HashMap::new();
    let segments: Vec<&str> = roles_claim.split('.').collect();

    if segments.len() == 1 {
        claims.insert(roles_claim.to_string(), roles_value);
    } else {
        let mut nested = roles_value;
        for segment in segments.iter().skip(1).rev() {
            let mut map = serde_json::Map::new();
            map.insert((*segment).to_string(), nested);
            nested = Value::Object(map);
        }
        claims.insert(segments[0].to_string(), nested);
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
fn provider_matrix_deeply_nested_claim_path() {
    let roles = extract_roles_for_provider("a.b.c", json!(["admin", "auditor"]));
    assert_eq!(roles, vec!["admin", "auditor"]);
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
