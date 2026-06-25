//! Unit tests for system compliance endpoint assembly logic

use crystal_forge::api::models::{
    ComplianceBundleSummary, ComplianceEnvironmentRef,
};
use crystal_forge::queries::compliance::{
    assemble_system_compliance_bundles, system_rollup, PolicyRow, SystemRow,
};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

fn make_system(hostname: &str, environment: Option<&str>) -> SystemRow {
    SystemRow {
        id: Uuid::new_v4(),
        hostname: hostname.to_string(),
        environment: environment.map(|s| s.to_string()),
        health_status: "healthy".to_string(),
        critical_cve_count: 0,
        high_cve_count: 0,
    }
}

fn make_bundle(id: Uuid, name: &str) -> ComplianceBundleSummary {
    ComplianceBundleSummary {
        id,
        name: name.to_string(),
        framework: "Test Framework".to_string(),
        version: "1.0".to_string(),
        description: None,
        layer: "infrastructure".to_string(),
        owner: "test-owner".to_string(),
        last_review: None,
        policy_ids: vec![],
        required_envs: vec![],
        control_count: 0,
        environment_count: 0,
    }
}

fn make_policy(bundle_id: Uuid, name: &str, enabled: bool) -> PolicyRow {
    PolicyRow {
        id: Uuid::new_v4(),
        bundle_id,
        name: name.to_string(),
        description: None,
        policy_type: "require_cf_agent".to_string(),
        config: serde_json::json!({}),
        enabled,
    }
}

/// Test that only applicable bundles are included in the result
#[test]
fn test_filters_non_applicable_bundles() {
    let system = make_system("test-host", Some("prod"));
    
    let bundle1_id = Uuid::new_v4();
    let bundle2_id = Uuid::new_v4();
    let bundle3_id = Uuid::new_v4();
    
    let bundle1 = make_bundle(bundle1_id, "Applicable Bundle 1");
    let bundle2 = make_bundle(bundle2_id, "Non-Applicable Bundle");
    let bundle3 = make_bundle(bundle3_id, "Applicable Bundle 2");
    
    let mut applicable_ids = HashSet::new();
    applicable_ids.insert(bundle1_id);
    applicable_ids.insert(bundle3_id);
    // bundle2 is intentionally not in the applicable set
    
    let policies_by_bundle = HashMap::new();
    
    let result = assemble_system_compliance_bundles(
        &system,
        vec![bundle1, bundle2, bundle3],
        &applicable_ids,
        &policies_by_bundle,
    );
    
    assert_eq!(result.len(), 2, "Should only include 2 applicable bundles");
    assert!(
        result.iter().any(|(b, _)| b.id == bundle1_id),
        "Should include bundle1"
    );
    assert!(
        result.iter().any(|(b, _)| b.id == bundle3_id),
        "Should include bundle3"
    );
    assert!(
        !result.iter().any(|(b, _)| b.id == bundle2_id),
        "Should NOT include bundle2"
    );
}

/// Test that bundles with no policies receive a valid rollup
#[test]
fn test_bundle_with_no_policies() {
    let system = make_system("test-host", Some("prod"));
    
    let bundle_id = Uuid::new_v4();
    let bundle = make_bundle(bundle_id, "Empty Bundle");
    
    let mut applicable_ids = HashSet::new();
    applicable_ids.insert(bundle_id);
    
    let policies_by_bundle = HashMap::new();
    
    let result = assemble_system_compliance_bundles(
        &system,
        vec![bundle],
        &applicable_ids,
        &policies_by_bundle,
    );
    
    assert_eq!(result.len(), 1);
    let (_, rollup) = &result[0];
    
    assert_eq!(rollup.hostname, "test-host");
    assert_eq!(rollup.total, 0, "No policies means zero total");
    assert_eq!(rollup.pass, 0);
    assert_eq!(rollup.warn, 0);
    assert_eq!(rollup.fail, 0);
    assert_eq!(rollup.score, 0, "Empty bundle should have 0 score");
}

/// Test that policies are correctly assigned to their bundle
#[test]
fn test_policies_grouped_by_bundle() {
    let system = make_system("test-host", Some("prod"));
    
    let bundle1_id = Uuid::new_v4();
    let bundle2_id = Uuid::new_v4();
    
    let bundle1 = make_bundle(bundle1_id, "Bundle 1");
    let bundle2 = make_bundle(bundle2_id, "Bundle 2");
    
    let mut applicable_ids = HashSet::new();
    applicable_ids.insert(bundle1_id);
    applicable_ids.insert(bundle2_id);
    
    let mut policies_by_bundle = HashMap::new();
    policies_by_bundle.insert(
        bundle1_id,
        vec![
            make_policy(bundle1_id, "policy1", true),
            make_policy(bundle1_id, "policy2", true),
        ],
    );
    policies_by_bundle.insert(
        bundle2_id,
        vec![make_policy(bundle2_id, "policy3", true)],
    );
    
    let result = assemble_system_compliance_bundles(
        &system,
        vec![bundle1, bundle2],
        &applicable_ids,
        &policies_by_bundle,
    );
    
    assert_eq!(result.len(), 2);
    
    let bundle1_rollup = result.iter().find(|(b, _)| b.id == bundle1_id).unwrap();
    let bundle2_rollup = result.iter().find(|(b, _)| b.id == bundle2_id).unwrap();
    
    assert_eq!(bundle1_rollup.1.total, 2, "Bundle 1 should have 2 policies");
    assert_eq!(bundle2_rollup.1.total, 1, "Bundle 2 should have 1 policy");
}

/// Test that system_rollup generates expected pass/warn/fail counts
#[test]
fn test_system_rollup_with_mixed_policies() {
    let system = make_system("healthy-host", Some("prod"));
    
    let bundle_id = Uuid::new_v4();
    let policies = vec![
        make_policy(bundle_id, "enabled-policy-1", true),
        make_policy(bundle_id, "enabled-policy-2", true),
        make_policy(bundle_id, "disabled-policy", false),
    ];
    
    let rollup = system_rollup(system, &policies);
    
    assert_eq!(rollup.total, 3, "Should count all policies");
    assert_eq!(rollup.pass, 2, "Enabled policies on healthy system should pass");
    assert_eq!(rollup.warn, 1, "Disabled policy should show as warn");
    assert_eq!(rollup.fail, 0);
    assert_eq!(rollup.evaluated_total, 2, "Only enabled policies evaluated");
    assert_eq!(rollup.score, 100, "All evaluated policies passed");
}

/// Test that empty assembly returns empty result
#[test]
fn test_empty_bundles_list() {
    let system = make_system("test-host", None);
    let applicable_ids = HashSet::new();
    let policies_by_bundle = HashMap::new();
    
    let result = assemble_system_compliance_bundles(
        &system,
        vec![],
        &applicable_ids,
        &policies_by_bundle,
    );
    
    assert_eq!(result.len(), 0, "Empty bundle list should produce empty result");
}

/// Test that rollup preserves system information
#[test]
fn test_rollup_preserves_system_info() {
    let system = make_system("test-host-123", Some("staging"));
    let policies = vec![make_policy(Uuid::new_v4(), "test-policy", true)];
    
    let rollup = system_rollup(system.clone(), &policies);
    
    assert_eq!(rollup.hostname, "test-host-123");
    assert_eq!(rollup.environment, Some("staging".to_string()));
    assert_eq!(rollup.system_id, system.id);
}
