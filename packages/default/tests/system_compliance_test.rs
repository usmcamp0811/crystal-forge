//! Unit tests for system compliance endpoint logic

use crystal_forge::api::models::{
    ComplianceBundleSummary, ComplianceEnvironmentRef, ComplianceSystemRollup,
    SystemComplianceBundle, SystemComplianceBundlesResponse,
};
use serde_json;
use uuid::Uuid;

/// Test that response serialization matches expected JSON structure
#[test]
fn test_response_serialization() {
    let system_id = Uuid::new_v4();
    let bundle_id = Uuid::new_v4();
    let env_id = Uuid::new_v4();

    let response = SystemComplianceBundlesResponse {
        system_id,
        bundles: vec![SystemComplianceBundle {
            bundle: ComplianceBundleSummary {
                id: bundle_id,
                name: "Test Bundle".to_string(),
                framework: "NIST".to_string(),
                version: "1.0".to_string(),
                description: Some("Test compliance bundle".to_string()),
                layer: "infrastructure".to_string(),
                owner: "security-team".to_string(),
                last_review: None,
                policy_ids: vec![],
                required_envs: vec![ComplianceEnvironmentRef {
                    id: env_id,
                    name: "prod".to_string(),
                    color_hex: "#ff0000".to_string(),
                }],
                control_count: 5,
                environment_count: 1,
            },
            rollup: ComplianceSystemRollup {
                system_id,
                hostname: "test-host".to_string(),
                environment: Some("prod".to_string()),
                applies: true,
                total: 5,
                evaluated_total: 5,
                pass: 4,
                warn: 1,
                fail: 0,
                waiver: 0,
                score: 80,
            },
        }],
    };

    let json = serde_json::to_string(&response).expect("serialization should succeed");
    assert!(json.contains("\"system_id\""));
    assert!(json.contains("\"bundles\""));
    assert!(json.contains("Test Bundle"));
    assert!(json.contains("\"score\":80"));
}

/// Test that response with no bundles serializes correctly
#[test]
fn test_empty_bundles_response() {
    let system_id = Uuid::new_v4();
    let response = SystemComplianceBundlesResponse {
        system_id,
        bundles: vec![],
    };

    let json = serde_json::to_string(&response).expect("serialization should succeed");
    assert!(json.contains("\"bundles\":[]"));
}

/// Test that response deserializes correctly
#[test]
fn test_response_deserialization() {
    let system_id = Uuid::new_v4();
    let bundle_id = Uuid::new_v4();

    let json = format!(
        r#"{{
            "system_id": "{}",
            "bundles": [{{
                "bundle": {{
                    "id": "{}",
                    "name": "Test Bundle",
                    "framework": "NIST",
                    "version": "1.0",
                    "description": null,
                    "layer": "infrastructure",
                    "owner": "security-team",
                    "last_review": null,
                    "policy_ids": [],
                    "required_envs": [],
                    "control_count": 5,
                    "environment_count": 0
                }},
                "rollup": {{
                    "system_id": "{}",
                    "hostname": "test-host",
                    "environment": "prod",
                    "applies": true,
                    "total": 5,
                    "evaluated_total": 5,
                    "pass": 4,
                    "warn": 1,
                    "fail": 0,
                    "waiver": 0,
                    "score": 80
                }}
            }}]
        }}"#,
        system_id, bundle_id, system_id
    );

    let response: SystemComplianceBundlesResponse =
        serde_json::from_str(&json).expect("deserialization should succeed");

    assert_eq!(response.system_id, system_id);
    assert_eq!(response.bundles.len(), 1);
    assert_eq!(response.bundles[0].bundle.name, "Test Bundle");
    assert_eq!(response.bundles[0].rollup.score, 80);
}

/// Test that the all-or-nothing model is correctly represented
#[test]
fn test_no_partial_error_field() {
    let system_id = Uuid::new_v4();
    let response = SystemComplianceBundlesResponse {
        system_id,
        bundles: vec![],
    };

    let json = serde_json::to_value(&response).expect("serialization should succeed");

    // Verify there is no "errors" field in the JSON
    assert!(json.get("errors").is_none(), "Response should not contain errors field - endpoint is all-or-nothing");
}
