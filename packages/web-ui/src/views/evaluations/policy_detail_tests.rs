//! Verifies UUID-bound detail alignment and compatibility with missing evidence.

use super::*;
use serde_json::json;
use uuid::Uuid;

fn fixture() -> EvalPolicyMatrixSnapshot {
    let ids = [Uuid::from_u128(3), Uuid::from_u128(1), Uuid::from_u128(2)];
    let columns: Vec<_> = ids
        .iter()
        .map(|id| {
            json!({
                "policy_version_id": id, "policy_id": Uuid::from_u128(10),
                "name": "Same policy name", "version": id.to_string()
            })
        })
        .collect();
    let bindings: Vec<_> = ids
        .iter()
        .map(|id| {
            json!({
                "policy_version_id": id, "report_only": false,
                "executable": true, "memberships": []
            })
        })
        .collect();
    serde_json::from_value(json!({
        "evaluation_attempt_id": null, "attempt_number": 2,
        "mock_evidence": false,
        "provenance": "captured", "columns": columns, "bundles": [],
        "empty_bundle_metadata_complete": true,
        "rows": [{
            "system_id": Uuid::from_u128(61), "system_name": "captured-host",
            "configuration_name": "nixosConfigurations.actual", "resolution": "resolved",
            "evaluation_digest": null, "policies": bindings,
            "cells": ["pass", "warn", "fail"],
            "details": [
                "config.security.audit: assertion passed",
                "config.services.openssh.settings: warning\nactual assertion cause",
                "config.networking.firewall.enable = false; expected true <not HTML>"
            ]
        }]
    }))
    .unwrap()
}

#[test]
fn details_follow_exact_versions_across_reordered_columns_not_names() {
    let mut snapshot = fixture();
    let matrix = captured(&snapshot).unwrap();
    let details = &matrix.rows[0].details;
    assert_eq!(details.len(), 3);
    for (index, column) in snapshot.columns.iter().enumerate() {
        assert_eq!(
            details.get(&column.policy_version_id.to_string()),
            snapshot.rows[0].details[index].as_ref()
        );
    }
    snapshot.columns.rotate_left(1);
    snapshot.rows[0].cells.rotate_left(1);
    snapshot.rows[0].details.rotate_left(1);
    let reordered = captured(&snapshot).unwrap();
    assert_eq!(&reordered.rows[0].details, details);
    assert_eq!(
        reordered.rows[0].details[&Uuid::from_u128(2).to_string()],
        "config.networking.firewall.enable = false; expected true <not HTML>"
    );
}

#[test]
fn short_null_and_omitted_detail_vectors_never_shift_evidence() {
    let mut snapshot = fixture();
    snapshot.rows[0].details[0] = None;
    snapshot.rows[0].details.truncate(2);
    let matrix = captured(&snapshot).unwrap();
    assert_eq!(matrix.rows[0].details.len(), 1);
    assert!(
        matrix.rows[0]
            .details
            .contains_key(&Uuid::from_u128(1).to_string())
    );
    let mut old = serde_json::to_value(snapshot).unwrap();
    old["rows"][0].as_object_mut().unwrap().remove("details");
    let old: EvalPolicyMatrixSnapshot = serde_json::from_value(old).unwrap();
    assert!(old.rows[0].details.is_empty());
    assert!(captured(&old).unwrap().rows[0].details.is_empty());
}

#[test]
fn unknown_nonassigned_and_skipped_implementations_have_no_detail() {
    let mut snapshot = fixture();
    snapshot.rows[0].cells = vec![
        EvalMatrixCell::Unknown,
        EvalMatrixCell::NotAssigned,
        EvalMatrixCell::Pass,
    ];
    snapshot.rows[0].policies.remove(1);
    let matrix = captured(&snapshot).unwrap();
    assert_eq!(matrix.rows[0].details.len(), 1);
    assert!(
        matrix.rows[0]
            .details
            .contains_key(&Uuid::from_u128(2).to_string())
    );
    snapshot.rows[0].policies[1].executable = false;
    assert!(captured(&snapshot).unwrap().rows[0].details.is_empty());
}
