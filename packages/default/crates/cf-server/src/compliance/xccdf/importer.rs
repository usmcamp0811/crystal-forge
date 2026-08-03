//! Validation and canonical record construction for committed XCCDF imports.
//!
//! This module is independent of HTTP and PostgreSQL. It converts a validated
//! [`XccdfImportPlan`] and a [`ParsedXccdf`] document into the canonical
//! record set needed for the persistence layer in `queries/compliance_interchange.rs`.

use std::collections::{HashMap, HashSet};
use uuid::Uuid;

use crate::compliance::xccdf::import_models::{
    ImportPlanError, ImportedPolicyRecord, ValidatedImportPlan, XccdfImportPlan,
    XccdfRuleImportAction,
};
use crate::compliance::xccdf::models::{DocumentClass, ParsedRule, ParsedXccdf};

// ── Document-class gate ───────────────────────────────────────────────────────

/// Returns `Some(error)` when the document class cannot be imported in this
/// slice.  CF-native documents and completely invalid documents are rejected.
pub fn check_document_class(parsed: &ParsedXccdf) -> Option<ImportPlanError> {
    match parsed.class {
        DocumentClass::ForeignXccdf => None, // supported
        DocumentClass::CfNativeExact | DocumentClass::CfNativeUnsupportedExtension => Some(
            ImportPlanError::document_class_unsupported(&format!("{:?}", parsed.class)),
        ),
        DocumentClass::InvalidXccdf => {
            // Blocking diagnostics have already been checked by the package
            // processor. An InvalidXccdf with no blocking errors can arrive
            // here when the document has only structural warnings. Reject it
            // to keep the import path conservative.
            Some(ImportPlanError::document_class_unsupported("invalid_xccdf"))
        }
        DocumentClass::UnsupportedPackage => Some(ImportPlanError::document_class_unsupported(
            "unsupported_package",
        )),
    }
}

// ── SHA-256 validation helpers ────────────────────────────────────────────────

/// Validate the expected_sha256 field.
///
/// * Must be a lowercase hex string of exactly 64 characters.
/// * Must match the actual package digest.
///
/// Returns `Some(Mismatch error)` when the digest does not match.
/// Returns `Some(Invalid error)` when the digest is syntactically wrong.
pub fn validate_sha256_match(expected: &str, actual: &str) -> Option<ImportPlanError> {
    if !is_sha256_hex(expected) {
        return Some(ImportPlanError::source_digest_invalid(expected));
    }
    if expected.to_ascii_lowercase() != actual.to_ascii_lowercase() {
        return Some(ImportPlanError::source_digest_mismatch(expected, actual));
    }
    None
}

fn is_sha256_hex(s: &str) -> bool {
    s.len() == 64
        && s.chars()
            .all(|c| matches!(c, '0'..='9' | 'a'..='f' | 'A'..='F'))
}

// ── Plan validation ───────────────────────────────────────────────────────────

/// Validate an import plan against the reparsed document.
///
/// Returns the validated plan with non-excluded rules in document order, or
/// the first validation error encountered.
pub fn validate_import_plan(
    plan: XccdfImportPlan,
    parsed: &ParsedXccdf,
) -> Result<ValidatedImportPlan, ImportPlanError> {
    // ── Bundle metadata ────────────────────────────────────────────────────
    if plan.bundle.name.trim().is_empty() {
        return Err(ImportPlanError::bundle_name_empty());
    }
    if plan.bundle.version.trim().is_empty() {
        return Err(ImportPlanError::bundle_version_empty());
    }
    if plan.bundle.framework.trim().is_empty() {
        return Err(ImportPlanError::bundle_framework_empty());
    }

    // ── Build rule index ───────────────────────────────────────────────────
    let rule_by_id: HashMap<&str, &ParsedRule> =
        parsed.rules.iter().map(|r| (r.id.as_str(), r)).collect();

    // ── selected_rule_ids: existence + uniqueness ──────────────────────────
    let mut seen_selected: HashSet<&str> = HashSet::new();
    for rule_id in &plan.selected_rule_ids {
        if !seen_selected.insert(rule_id.as_str()) {
            return Err(ImportPlanError::rule_duplicate(rule_id));
        }
        if !rule_by_id.contains_key(rule_id.as_str()) {
            return Err(ImportPlanError::rule_not_found(rule_id));
        }
    }

    // ── rule_actions: uniqueness + only referencing selected rules ─────────
    let mut action_by_rule_id: HashMap<&str, &XccdfRuleImportAction> = HashMap::new();
    for action in &plan.rule_actions {
        let rule_id = action.rule_id();
        if action_by_rule_id.insert(rule_id, action).is_some() {
            return Err(ImportPlanError::action_duplicate(rule_id));
        }
        if !seen_selected.contains(rule_id) {
            return Err(ImportPlanError::action_for_unselected(rule_id));
        }
    }

    // ── Every selected rule must have exactly one action ───────────────────
    for rule_id in &plan.selected_rule_ids {
        if !action_by_rule_id.contains_key(rule_id.as_str()) {
            return Err(ImportPlanError::action_missing(rule_id));
        }
    }

    // ── Profile validation ─────────────────────────────────────────────────
    let profile_rule_set: Option<HashSet<&str>> = if let Some(ref pid) = plan.selected_profile_id {
        let profile = parsed
            .profiles
            .iter()
            .find(|p| p.id.as_str() == pid.as_str());
        let Some(profile) = profile else {
            return Err(ImportPlanError::profile_not_found(pid));
        };
        Some(profile.select_ids.iter().map(String::as_str).collect())
    } else {
        None
    };

    if let (Some(profile_ids), Some(pid)) = (&profile_rule_set, &plan.selected_profile_id) {
        for rule_id in &plan.selected_rule_ids {
            if !profile_ids.contains(rule_id.as_str()) {
                let action = action_by_rule_id[rule_id.as_str()];
                if !action.is_exclude() {
                    return Err(ImportPlanError::rule_not_in_profile(rule_id, pid));
                }
            }
        }
    }

    // ── Collect non-excluded rules in document order ───────────────────────
    let selected_set = &seen_selected;
    let mut rules_to_import: Vec<(ParsedRule, XccdfRuleImportAction)> = parsed
        .rules
        .iter()
        .filter(|r| selected_set.contains(r.id.as_str()))
        .map(|r| {
            let action = action_by_rule_id[r.id.as_str()].clone();
            (r.clone(), action)
        })
        .collect();

    // Stable document order is already guaranteed by iterating parsed.rules.

    Ok(ValidatedImportPlan {
        expected_sha256: plan.expected_sha256,
        bundle: plan.bundle,
        rules_to_import,
    })
}

// ── Canonical record construction ─────────────────────────────────────────────

/// Build an [`ImportedPolicyRecord`] for each non-excluded rule.
pub fn build_policy_records(validated: &ValidatedImportPlan) -> Vec<ImportedPolicyRecord> {
    validated
        .rules_to_import
        .iter()
        .enumerate()
        .filter_map(|(order_in_selected, (rule, action))| {
            let impl_state = action.implementation_state()?; // None = Exclude

            let name = rule
                .title
                .clone()
                .filter(|t| !t.trim().is_empty())
                .unwrap_or_else(|| rule.id.clone());

            let compliance_metadata = ImportedPolicyRecord::build_compliance_metadata(rule);

            // For opaque rules, preserve the full XML fragment when available.
            let opaque_xml = if impl_state == "opaque" {
                rule.preserved_xml.clone()
            } else {
                None
            };

            Some(ImportedPolicyRecord {
                policy_id: Uuid::new_v4(),
                policy_version_id: Uuid::new_v4(),
                source_rule_id: rule.id.clone(),
                source_rule_order: rule.rule_order.unwrap_or(order_in_selected),
                implementation_state: impl_state,
                name,
                description: rule.description.clone(),
                compliance_metadata,
                opaque_xml,
            })
        })
        .collect()
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compliance::xccdf::import_models::{ImportedBundlePlan, XccdfImportPlan};
    use crate::compliance::xccdf::models::{DocumentClass, Fidelity, ParsedXccdf};

    fn minimal_foreign_parsed(rule_ids: &[&str]) -> ParsedXccdf {
        use crate::compliance::xccdf::models::{BenchmarkMeta, ParsedRule};
        ParsedXccdf {
            class: DocumentClass::ForeignXccdf,
            fidelity: Fidelity::PreservedOpaque,
            fidelity_losses: vec![],
            source_filename: Some("test.xml".into()),
            source_bytes: vec![],
            source_sha256: "a".repeat(64),
            xccdf_namespace_version: Some("1.2"),
            xccdf_version: Some("1.0".into()),
            benchmark: Some(BenchmarkMeta {
                id: "xccdf_test_benchmark".into(),
                title: Some("Test".into()),
                description: None,
                version: Some("1.0".into()),
                status: Some("draft".into()),
                status_date: None,
                platforms: vec![],
                publisher: None,
                references: vec![],
            }),
            profiles: vec![],
            rules: rule_ids
                .iter()
                .enumerate()
                .map(|(i, id)| ParsedRule {
                    id: id.to_string(),
                    title: Some(format!("Rule {}", id)),
                    description: None,
                    rationale: None,
                    severity: Some("medium".into()),
                    weight: None,
                    version: None,
                    checks: vec![],
                    fix: None,
                    identifiers: vec![],
                    references: vec![],
                    platforms: vec![],
                    group_id: None,
                    rule_order: Some(i),
                    cf_policy_meta: None,
                    preserved_xml: None,
                })
                .collect(),
            groups: vec![],
            values: vec![],
            cf_bundle_meta: None,
            signature_info: None,
            errors: vec![],
            warnings: vec![],
        }
    }

    fn valid_plan(rule_ids: &[&str]) -> XccdfImportPlan {
        XccdfImportPlan {
            expected_sha256: "a".repeat(64),
            selected_profile_id: None,
            selected_rule_ids: rule_ids.iter().map(|s| s.to_string()).collect(),
            rule_actions: rule_ids
                .iter()
                .map(|id| XccdfRuleImportAction::CreateManual {
                    rule_id: id.to_string(),
                })
                .collect(),
            bundle: ImportedBundlePlan {
                name: "Test Bundle".into(),
                framework: "TEST".into(),
                version: "1.0".into(),
                layer: None,
                owner: None,
                description: None,
            },
        }
    }

    #[test]
    fn valid_foreign_plan_succeeds() {
        let parsed = minimal_foreign_parsed(&["rule-1", "rule-2"]);
        let plan = valid_plan(&["rule-1", "rule-2"]);
        let result = validate_import_plan(plan, &parsed);
        assert!(
            result.is_ok(),
            "valid plan should succeed: {:?}",
            result.err()
        );
        let validated = result.unwrap();
        assert_eq!(validated.rules_to_import.len(), 2);
    }

    #[test]
    fn invalid_sha256_syntax_is_rejected_by_sha256_validator() {
        // validate_import_plan does NOT validate digest syntax —
        // the handler calls validate_sha256_match first.
        // Verify that validate_sha256_match catches bad syntax.
        let err = validate_sha256_match("not-a-sha256", &"a".repeat(64));
        assert!(matches!(
            err,
            Some(ImportPlanError {
                code: "SOURCE_DIGEST_INVALID",
                ..
            })
        ));
    }

    #[test]
    fn sha256_mismatch_is_detected_by_validate_sha256_match() {
        // validate_import_plan does NOT check the digest against upload bytes —
        // the handler does. validate_sha256_match is the dedicated function.
        let err = validate_sha256_match(&"a".repeat(64), &"b".repeat(64));
        assert!(err.is_some());
        let err = err.unwrap();
        assert_eq!(err.code, "SOURCE_DIGEST_MISMATCH");
    }

    #[test]
    fn digest_invalid_syntax() {
        let err = validate_sha256_match("not-hex", &"a".repeat(64));
        assert!(matches!(
            err,
            Some(ImportPlanError {
                code: "SOURCE_DIGEST_INVALID",
                ..
            })
        ));
    }

    #[test]
    fn digest_match_returns_none() {
        let digest = "a".repeat(64);
        assert!(validate_sha256_match(&digest, &digest).is_none());
    }

    #[test]
    fn duplicate_selected_rule_is_rejected() {
        let parsed = minimal_foreign_parsed(&["r1", "r2"]);
        let mut plan = valid_plan(&["r1"]);
        plan.selected_rule_ids = vec!["r1".into(), "r1".into()];
        plan.rule_actions = vec![XccdfRuleImportAction::CreateManual {
            rule_id: "r1".into(),
        }];
        let result = validate_import_plan(plan, &parsed);
        assert!(matches!(
            result,
            Err(ImportPlanError {
                code: "IMPORT_RULE_DUPLICATE",
                ..
            })
        ));
    }

    #[test]
    fn rule_not_in_document_is_rejected() {
        let parsed = minimal_foreign_parsed(&["r1"]);
        let plan = valid_plan(&["r99"]);
        let result = validate_import_plan(plan, &parsed);
        assert!(matches!(
            result,
            Err(ImportPlanError {
                code: "IMPORT_RULE_NOT_FOUND",
                ..
            })
        ));
    }

    #[test]
    fn duplicate_action_is_rejected() {
        let parsed = minimal_foreign_parsed(&["r1"]);
        let mut plan = valid_plan(&["r1"]);
        plan.rule_actions = vec![
            XccdfRuleImportAction::CreateManual {
                rule_id: "r1".into(),
            },
            XccdfRuleImportAction::CreateUnbound {
                rule_id: "r1".into(),
            },
        ];
        let result = validate_import_plan(plan, &parsed);
        assert!(matches!(
            result,
            Err(ImportPlanError {
                code: "IMPORT_RULE_ACTION_DUPLICATE",
                ..
            })
        ));
    }

    #[test]
    fn missing_action_for_selected_rule_is_rejected() {
        let parsed = minimal_foreign_parsed(&["r1", "r2"]);
        let mut plan = valid_plan(&["r1", "r2"]);
        plan.rule_actions = vec![XccdfRuleImportAction::CreateManual {
            rule_id: "r1".into(),
        }]; // r2 missing
        let result = validate_import_plan(plan, &parsed);
        assert!(matches!(
            result,
            Err(ImportPlanError {
                code: "IMPORT_RULE_ACTION_MISSING",
                ..
            })
        ));
    }

    #[test]
    fn action_for_unselected_rule_is_rejected() {
        let parsed = minimal_foreign_parsed(&["r1", "r99"]);
        let mut plan = valid_plan(&["r1"]);
        // Add action for r99 which is not in selected_rule_ids.
        plan.rule_actions.push(XccdfRuleImportAction::CreateManual {
            rule_id: "r99".into(),
        });
        let result = validate_import_plan(plan, &parsed);
        assert!(matches!(
            result,
            Err(ImportPlanError {
                code: "IMPORT_RULE_ACTION_DUPLICATE",
                ..
            })
        ));
    }

    #[test]
    fn profile_not_found_is_rejected() {
        let parsed = minimal_foreign_parsed(&["r1"]);
        let mut plan = valid_plan(&["r1"]);
        plan.selected_profile_id = Some("xccdf_missing_profile".into());
        let result = validate_import_plan(plan, &parsed);
        assert!(matches!(
            result,
            Err(ImportPlanError {
                code: "IMPORT_PROFILE_NOT_FOUND",
                ..
            })
        ));
    }

    #[test]
    fn rule_outside_profile_is_rejected() {
        use crate::compliance::xccdf::models::ParsedProfile;
        let mut parsed = minimal_foreign_parsed(&["r1", "r2"]);
        parsed.profiles.push(ParsedProfile {
            id: "xccdf_profile_a".into(),
            title: Some("Profile A".into()),
            description: None,
            select_ids: vec!["r1".into()], // only r1 is in the profile
            extends_id: None,
            is_abstract: false,
            is_baseline: false,
        });
        let mut plan = valid_plan(&["r2"]);
        plan.selected_profile_id = Some("xccdf_profile_a".into());
        let result = validate_import_plan(plan, &parsed);
        assert!(matches!(
            result,
            Err(ImportPlanError {
                code: "IMPORT_RULE_NOT_IN_PROFILE",
                ..
            })
        ));
    }

    #[test]
    fn cf_native_document_is_rejected_as_unsupported() {
        let mut parsed = minimal_foreign_parsed(&["r1"]);
        parsed.class = DocumentClass::CfNativeExact;
        let err = check_document_class(&parsed);
        assert!(err.is_some());
        assert_eq!(err.unwrap().code, "IMPORT_DOCUMENT_CLASS_UNSUPPORTED");
    }

    #[test]
    fn excluded_rules_produce_no_policy_records() {
        let parsed = minimal_foreign_parsed(&["r1", "r2"]);
        let plan = XccdfImportPlan {
            expected_sha256: "a".repeat(64),
            selected_profile_id: None,
            selected_rule_ids: vec!["r1".into(), "r2".into()],
            rule_actions: vec![
                XccdfRuleImportAction::CreateManual {
                    rule_id: "r1".into(),
                },
                XccdfRuleImportAction::Exclude {
                    rule_id: "r2".into(),
                },
            ],
            bundle: ImportedBundlePlan {
                name: "Bundle".into(),
                framework: "FW".into(),
                version: "1.0".into(),
                layer: None,
                owner: None,
                description: None,
            },
        };
        let validated = validate_import_plan(plan, &parsed).unwrap();
        let records = build_policy_records(&validated);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].source_rule_id, "r1");
        assert_eq!(records[0].implementation_state, "manual");
    }

    #[test]
    fn bundle_name_empty_is_rejected() {
        let parsed = minimal_foreign_parsed(&["r1"]);
        let mut plan = valid_plan(&["r1"]);
        plan.bundle.name = "  ".into();
        let result = validate_import_plan(plan, &parsed);
        assert!(matches!(
            result,
            Err(ImportPlanError {
                code: "IMPORT_PLAN_INVALID",
                ..
            })
        ));
    }

    #[test]
    fn bundle_version_empty_is_rejected() {
        let parsed = minimal_foreign_parsed(&["r1"]);
        let mut plan = valid_plan(&["r1"]);
        plan.bundle.version = "".into();
        let result = validate_import_plan(plan, &parsed);
        assert!(matches!(
            result,
            Err(ImportPlanError {
                code: "IMPORT_PLAN_INVALID",
                ..
            })
        ));
    }
}
