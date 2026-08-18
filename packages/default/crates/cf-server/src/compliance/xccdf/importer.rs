//! Validation and canonical record construction for committed XCCDF imports.
//!
//! This module is independent of HTTP and PostgreSQL. It converts a validated
//! [`XccdfImportPlan`] and a [`ParsedXccdf`] document into the canonical
//! record set needed for the persistence layer in `queries/compliance_interchange.rs`.

use std::collections::{HashMap, HashSet};
use uuid::Uuid;

use crate::compliance::digest::{
    BundleMembershipEntry, BundleVersionCanonical, PolicyVersionCanonical,
};
use crate::compliance::interchange::{CANONICALIZATION_VERSION, DIGEST_ALGORITHM};
use crate::compliance::xccdf::import_models::{
    ImportPlanError, ImportedPolicyRecord, MapExistingProof, ValidatedImportPlan, XccdfImportPlan,
    XccdfRuleImportAction,
};
use crate::compliance::xccdf::models::{DocumentClass, ParsedRule, ParsedXccdf};

// ── Document-class gate ───────────────────────────────────────────────────────

/// Returns `Some(error)` when the document class cannot be imported in this
/// slice.  CF-native documents and completely invalid documents are rejected.
pub fn check_document_class(parsed: &ParsedXccdf) -> Option<ImportPlanError> {
    match parsed.class {
        DocumentClass::ForeignXccdf => None, // supported
        DocumentClass::CfNativeExact => validate_cf_native_document(parsed).err(),
        DocumentClass::CfNativeUnsupportedExtension => Some(ImportPlanError::cf_native_invalid(
            "CF_NATIVE_PROFILE_UNSUPPORTED",
            "the document uses unsupported Crystal Forge extension content",
        )),
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

/// Validate the complete CF-native contract and construct portable records.
/// This is intentionally typed and digest-checking: a document that claims to
/// be CF-native is never silently downgraded to the foreign import path.
pub fn validate_cf_native_document(
    parsed: &ParsedXccdf,
) -> Result<(ValidatedImportPlan, Vec<ImportedPolicyRecord>), ImportPlanError> {
    let bundle = parsed.cf_bundle_meta.as_ref().ok_or_else(|| {
        ImportPlanError::cf_native_invalid(
            "CF_NATIVE_METADATA_INVALID",
            "CF-native document is missing required bundle metadata",
        )
    })?;
    if bundle.bundle_id.is_nil() || bundle.bundle_version_id.is_nil() {
        return Err(ImportPlanError::cf_native_invalid(
            "CF_NATIVE_METADATA_INVALID",
            "CF-native bundle identities must be valid UUIDs",
        ));
    }
    if bundle.schema_version.as_deref() != Some("1") {
        return Err(ImportPlanError::cf_native_invalid(
            "CF_NATIVE_PROFILE_UNSUPPORTED",
            "unsupported or missing Crystal Forge bundle schema version",
        ));
    }
    if !matches!(
        bundle.publication_state.as_str(),
        "incomplete" | "draft" | "interim" | "accepted" | "deprecated"
    ) {
        return Err(ImportPlanError::cf_native_invalid(
            "CF_NATIVE_METADATA_INVALID",
            "invalid bundle publication state",
        ));
    }
    if bundle.digest_algorithm.as_deref() != Some(DIGEST_ALGORITHM) {
        return Err(ImportPlanError::cf_native_invalid(
            "CF_NATIVE_DIGEST_ALGORITHM_UNSUPPORTED",
            "CF-native bundle digest algorithm must be sha-256",
        ));
    }
    if bundle.canonicalization_version.as_deref() != Some(CANONICALIZATION_VERSION) {
        return Err(ImportPlanError::cf_native_invalid(
            "CF_NATIVE_CANONICALIZATION_UNSUPPORTED",
            "CF-native bundle canonicalization must be cf-model-json-1",
        ));
    }
    let benchmark = parsed.benchmark.as_ref().ok_or_else(|| {
        ImportPlanError::cf_native_invalid("CF_NATIVE_METADATA_INVALID", "missing XCCDF benchmark")
    })?;
    let mut records = Vec::with_capacity(parsed.rules.len());
    let mut rules = Vec::with_capacity(parsed.rules.len());
    for (order, rule) in parsed.rules.iter().enumerate() {
        let meta = rule.cf_policy_meta.as_ref().ok_or_else(|| {
            ImportPlanError::cf_native_invalid(
                "CF_NATIVE_METADATA_INVALID",
                format!("rule {} is missing CF policy identity", rule.id),
            )
        })?;
        if meta.policy_id.is_nil() || meta.policy_version_id.is_nil() {
            return Err(ImportPlanError::cf_native_invalid(
                "CF_NATIVE_METADATA_INVALID",
                format!("rule {} has invalid portable identity", rule.id),
            ));
        }
        let policy_type = meta.policy_type.clone().ok_or_else(|| {
            ImportPlanError::cf_native_invalid(
                "CF_NATIVE_POLICY_TYPE_UNSUPPORTED",
                format!("rule {} has no typed CF policy implementation", rule.id),
            )
        })?;
        let implementation_state = meta.implementation_state.clone().ok_or_else(|| {
            ImportPlanError::cf_native_invalid(
                "CF_NATIVE_PAYLOAD_INVALID",
                format!("rule {} has no implementation state", rule.id),
            )
        })?;
        let enabled_default = meta.enabled_default.ok_or_else(|| {
            ImportPlanError::cf_native_invalid(
                "CF_NATIVE_METADATA_INVALID",
                format!("rule {} has invalid enabled-default metadata", rule.id),
            )
        })?;
        let selected = meta.selected.ok_or_else(|| {
            ImportPlanError::cf_native_invalid(
                "CF_NATIVE_METADATA_INVALID",
                format!("rule {} has invalid selected metadata", rule.id),
            )
        })?;
        let policy_order = meta.policy_order.ok_or_else(|| {
            ImportPlanError::cf_native_invalid(
                "CF_NATIVE_METADATA_INVALID",
                format!("rule {} has invalid policy-order metadata", rule.id),
            )
        })?;
        if !matches!(
            implementation_state.as_str(),
            "native" | "manual" | "external" | "unbound" | "opaque"
        ) {
            return Err(ImportPlanError::cf_native_invalid(
                "CF_NATIVE_PAYLOAD_INVALID",
                format!("rule {} has invalid implementation state", rule.id),
            ));
        }
        if meta.digest_algorithm.as_deref() != Some(DIGEST_ALGORITHM)
            || meta.canonicalization_version.as_deref() != Some(CANONICALIZATION_VERSION)
        {
            return Err(ImportPlanError::cf_native_invalid(
                "CF_NATIVE_DIGEST_ALGORITHM_UNSUPPORTED",
                format!("rule {} has unsupported digest contract", rule.id),
            ));
        }
        let imported_digest = meta.digest.clone().ok_or_else(|| {
            ImportPlanError::cf_native_invalid(
                "CF_NATIVE_METADATA_INVALID",
                format!("rule {} is missing semantic digest", rule.id),
            )
        })?;
        let compliance_metadata = meta
            .compliance_metadata
            .clone()
            .unwrap_or_else(|| ImportedPolicyRecord::build_compliance_metadata(rule, false));
        let config = meta.config.clone().ok_or_else(|| {
            ImportPlanError::cf_native_invalid(
                "CF_NATIVE_PAYLOAD_INVALID",
                format!("rule {} is missing typed configuration", rule.id),
            )
        })?;
        let dependencies = meta
            .dependencies
            .clone()
            .unwrap_or_else(|| serde_json::json!([]));
        let canonical = PolicyVersionCanonical {
            name: rule.title.clone().unwrap_or_else(|| rule.id.clone()),
            description: rule.description.clone(),
            policy_type: policy_type.clone(),
            implementation_state: implementation_state.clone(),
            execution_phase: meta.execution_phase.clone().ok_or_else(|| {
                ImportPlanError::cf_native_invalid(
                    "CF_NATIVE_PAYLOAD_INVALID",
                    format!("rule {} is missing execution phase", rule.id),
                )
            })?,
            config: config.clone(),
            compliance_metadata: compliance_metadata.clone(),
            dependencies: dependencies.clone(),
            opaque_xml_digest: PolicyVersionCanonical::digest_opaque_xml(
                rule.preserved_xml.as_deref(),
            ),
            enabled_by_default: Some(enabled_default),
        };
        let recalculated_digest = canonical.compute_digest();
        if recalculated_digest != imported_digest {
            return Err(ImportPlanError::cf_native_invalid(
                "CF_NATIVE_DIGEST_MISMATCH",
                format!(
                    "rule {} semantic digest does not match its typed payload (imported {}, recalculated {})",
                    rule.id, imported_digest, recalculated_digest
                ),
            ));
        }
        let version = meta.version.clone();
        records.push(ImportedPolicyRecord {
            policy_id: meta.policy_id,
            policy_version_id: meta.policy_version_id,
            source_rule_id: rule.id.clone(),
            source_rule_order: rule.rule_order.unwrap_or(order),
            implementation_state,
            policy_type,
            version,
            execution_phase: canonical.execution_phase.clone(),
            config,
            dependencies,
            enabled_by_default: enabled_default,
            portable: true,
            semantic_digest: Some(imported_digest),
            selected,
            policy_order,
            name: canonical.name,
            description: canonical.description,
            compliance_metadata,
            opaque_xml: rule.preserved_xml.clone(),
            mapped_policy_version_id: None,
            mapped_policy_proof: None,
            mapping_semantics: None,
            evidence_requirements: Vec::new(),
        });
        rules.push((
            rule.clone(),
            XccdfRuleImportAction::CreateUnbound {
                rule_id: rule.id.clone(),
                customization: Default::default(),
            },
        ));
    }
    let mut ordered_records: Vec<&ImportedPolicyRecord> = records.iter().collect();
    ordered_records.sort_by_key(|record| record.policy_order);
    let members = ordered_records
        .into_iter()
        .map(|record| BundleMembershipEntry {
            policy_version_id: record.policy_version_id,
            selected: record.selected,
        })
        .collect();
    let bundle_canonical = BundleVersionCanonical {
        name: benchmark
            .title
            .clone()
            .unwrap_or_else(|| bundle.bundle_id.to_string()),
        framework: bundle.framework.clone().ok_or_else(|| {
            ImportPlanError::cf_native_invalid(
                "CF_NATIVE_METADATA_INVALID",
                "missing bundle framework",
            )
        })?,
        framework_version: bundle.framework_version.clone(),
        description: benchmark.description.clone(),
        layer: bundle.layer.clone().unwrap_or_else(|| "fleet".into()),
        owner: bundle
            .owner
            .clone()
            .unwrap_or_else(|| "Platform Security".into()),
        members,
    };
    if bundle.digest.as_deref() != Some(bundle_canonical.compute_digest().as_str()) {
        return Err(ImportPlanError::cf_native_invalid(
            "CF_NATIVE_DIGEST_MISMATCH",
            format!(
                "bundle semantic digest does not match its typed payload (imported {}, recalculated {})",
                bundle.digest.as_deref().unwrap_or(""),
                bundle_canonical.compute_digest()
            ),
        ));
    }
    let validated = ValidatedImportPlan {
        expected_sha256: parsed.source_sha256.clone(),
        bundle: crate::compliance::xccdf::import_models::ImportedBundlePlan {
            name: bundle_canonical.name,
            framework: bundle_canonical.framework,
            version: benchmark.version.clone().unwrap_or_else(|| "1".into()),
            layer: Some(bundle_canonical.layer),
            owner: Some(bundle_canonical.owner),
            description: bundle_canonical.description,
            environment_ids: Vec::new(),
        },
        is_disa_stig: false,
        rules_to_import: rules,
        mapping_semantics: std::collections::HashMap::new(),
        shared_group_decisions: Vec::new(),
    };
    Ok((validated, records))
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
        if let XccdfRuleImportAction::CreateNativeCustom { custom_check, .. } = action {
            if !matches!(custom_check.mode.as_str(), "all" | "any") {
                return Err(ImportPlanError::native_check_invalid(
                    rule_id,
                    "mode must be 'all' or 'any'",
                ));
            }
            if custom_check.rules.is_empty() {
                return Err(ImportPlanError::native_check_invalid(
                    rule_id,
                    "at least one assertion is required",
                ));
            }
            for assertion in &custom_check.rules {
                if assertion.field_name.trim().is_empty()
                    || assertion.expression.trim().is_empty()
                    || assertion.description.trim().is_empty()
                {
                    return Err(ImportPlanError::native_check_invalid(
                        rule_id,
                        "assertion field_name, expression, and description are required",
                    ));
                }
                if !assertion.expression.contains("config.")
                    || assertion.expression.contains("cfg.config.")
                {
                    return Err(ImportPlanError::native_check_invalid(
                        rule_id,
                        "assertion expression must use the canonical config binding",
                    ));
                }
            }
        }
    }

    // ── Every selected rule must have exactly one action ───────────────────
    for rule_id in &plan.selected_rule_ids {
        if !action_by_rule_id.contains_key(rule_id.as_str()) {
            return Err(ImportPlanError::action_missing(rule_id));
        }
    }

    // Related-candidate evidence is only meaningful as provenance for an
    // explicitly reviewed MapExisting selection. It must never be attached to
    // a newly authored policy or combined with deterministic proof.
    for (rule_id, semantics) in &plan.mapping_semantics {
        let Some(reviewed) = semantics.reviewed_related_candidate.as_ref() else {
            continue;
        };
        let Some(action) = action_by_rule_id.get(rule_id.as_str()) else {
            return Err(ImportPlanError::cf_native_invalid(
                "IMPORT_RELATED_REVIEW_INVALID",
                format!("reviewed related candidate references unknown rule {rule_id}"),
            ));
        };
        let XccdfRuleImportAction::MapExisting {
            policy_version_id,
            proof,
            ..
        } = action
        else {
            return Err(ImportPlanError::cf_native_invalid(
                "IMPORT_RELATED_REVIEW_INVALID",
                format!("reviewed related candidate for {rule_id} requires MapExisting"),
            ));
        };
        if proof.is_some() || *policy_version_id != reviewed.policy_version_id {
            return Err(ImportPlanError::cf_native_invalid(
                "IMPORT_RELATED_REVIEW_INVALID",
                format!("reviewed related candidate for {rule_id} must match unproved MapExisting"),
            ));
        }
        if !matches!(
            semantics.relationship.as_deref(),
            Some("implements" | "supports" | "provides_evidence_for")
        ) || !matches!(semantics.coverage.as_deref(), Some("full" | "partial"))
        {
            return Err(ImportPlanError::cf_native_invalid(
                "IMPORT_RELATED_REVIEW_INVALID",
                format!(
                    "reviewed related candidate for {rule_id} requires explicit relationship and coverage"
                ),
            ));
        }
    }

    // Exact technical reuse is deterministic evidence for policy selection,
    // but it does not establish that the whole compliance requirement is fully
    // implemented. Require the reviewer to state the mapping semantics rather
    // than silently applying implements/full defaults.
    for action in &plan.rule_actions {
        let XccdfRuleImportAction::MapExisting {
            rule_id,
            proof: Some(MapExistingProof::ExactTechnicalMatch),
            ..
        } = action
        else {
            continue;
        };
        let Some(semantics) = plan.mapping_semantics.get(rule_id) else {
            return Err(ImportPlanError::cf_native_invalid(
                "IMPORT_MAPPING_SEMANTICS_INVALID",
                format!(
                    "exact technical reuse for {rule_id} requires explicit relationship and coverage"
                ),
            ));
        };
        if !matches!(
            semantics.relationship.as_deref(),
            Some("implements" | "supports" | "provides_evidence_for")
        ) || !matches!(semantics.coverage.as_deref(), Some("full" | "partial"))
        {
            return Err(ImportPlanError::cf_native_invalid(
                "IMPORT_MAPPING_SEMANTICS_INVALID",
                format!(
                    "exact technical reuse for {rule_id} requires explicit relationship and coverage"
                ),
            ));
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
    let rules_to_import: Vec<(ParsedRule, XccdfRuleImportAction)> = parsed
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
        is_disa_stig: parsed.class == DocumentClass::ForeignXccdf
            && parsed
                .benchmark
                .as_ref()
                .and_then(|benchmark| benchmark.publisher.as_deref())
                .is_some_and(|publisher| publisher.eq_ignore_ascii_case("DISA")),
        rules_to_import,
        mapping_semantics: plan.mapping_semantics,
        shared_group_decisions: plan.shared_group_decisions,
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
            let impl_state = action.implementation_state()?.to_owned(); // None = Exclude

            let customization = action.customization();
            let name = customization
                .and_then(|c| c.policy_name.clone())
                .filter(|t| !t.trim().is_empty())
                .or_else(|| rule.title.clone().filter(|t| !t.trim().is_empty()))
                .unwrap_or_else(|| rule.id.clone());

            let mut compliance_metadata =
                ImportedPolicyRecord::build_compliance_metadata(rule, validated.is_disa_stig);
            let (policy_type, execution_phase, config, evidence_requirements) = match action {
                XccdfRuleImportAction::CreateNativeCustom {
                    custom_check,
                    evidence_requirements,
                    ..
                } => (
                    "custom_check".to_string(),
                    "nix-evaluation".to_string(),
                    serde_json::to_value(custom_check).unwrap_or_else(|_| serde_json::json!({})),
                    evidence_requirements.clone(),
                ),
                XccdfRuleImportAction::CreateManual {
                    evidence_requirements,
                    ..
                } => (
                    "imported_xccdf".to_string(),
                    "not-applicable".to_string(),
                    serde_json::json!({}),
                    evidence_requirements.clone(),
                ),
                _ => (
                    "imported_xccdf".to_string(),
                    "not-applicable".to_string(),
                    serde_json::json!({}),
                    Vec::new(),
                ),
            };
            if !evidence_requirements.is_empty() {
                compliance_metadata["evidence_requirements"] =
                    serde_json::to_value(&evidence_requirements)
                        .unwrap_or_else(|_| serde_json::json!([]));
            }
            if let Some(note) = customization.and_then(|c| c.implementation_note.clone()) {
                compliance_metadata["implementation_note"] = serde_json::Value::String(note);
            }
            if let Some(severity) = customization
                .and_then(|c| c.policy_severity.clone())
                .filter(|value| matches!(value.as_str(), "high" | "medium" | "low"))
            {
                compliance_metadata["policy_severity"] = serde_json::Value::String(severity);
            }
            if let Some(rationale) = customization.and_then(|c| c.policy_rationale.clone()) {
                compliance_metadata["policy_rationale"] = serde_json::Value::String(rationale);
            }

            // For opaque rules, preserve the full XML fragment when available.
            let opaque_xml = if impl_state == "opaque" {
                rule.preserved_xml.clone()
            } else {
                None
            };

            let description = customization
                .and_then(|c| c.policy_description.clone())
                .or_else(|| rule.description.clone());
            let (mapped_policy_version_id, mapped_policy_proof) = match action {
                XccdfRuleImportAction::MapExisting {
                    policy_version_id,
                    proof,
                    ..
                } => (Some(*policy_version_id), *proof),
                _ => (None, None),
            };
            let mapping_semantics = validated.mapping_semantics.get(&rule.id).cloned();

            Some(ImportedPolicyRecord {
                policy_id: Uuid::new_v4(),
                policy_version_id: Uuid::new_v4(),
                source_rule_id: rule.id.clone(),
                source_rule_order: rule.rule_order.unwrap_or(order_in_selected),
                implementation_state: impl_state,
                policy_type,
                version: None,
                execution_phase,
                config,
                dependencies: serde_json::json!([]),
                enabled_by_default: false,
                portable: false,
                semantic_digest: None,
                selected: true,
                policy_order: rule.rule_order.unwrap_or(order_in_selected) as i32,
                name,
                description,
                compliance_metadata,
                opaque_xml,
                mapped_policy_version_id,
                mapped_policy_proof,
                mapping_semantics,
                evidence_requirements,
            })
        })
        .collect()
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compliance::xccdf::import_models::{
        ImportedBundlePlan, ImportedCustomCheck, ImportedCustomCheckRule, ImportedMappingSemantics,
        ImportedPolicyCustomization, MapExistingProof, XccdfImportPlan, XccdfRuleImportAction,
    };
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
                    customization: Default::default(),
                    evidence_requirements: Vec::new(),
                })
                .collect(),
            mapping_semantics: std::collections::HashMap::new(),

            shared_group_decisions: Vec::new(),

            bundle: ImportedBundlePlan {
                name: "Test Bundle".into(),
                framework: "TEST".into(),
                version: "1.0".into(),
                layer: None,
                owner: None,
                description: None,
                environment_ids: Vec::new(),
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
    fn disa_stig_source_metadata_projects_only_supported_classification() {
        let mut parsed = minimal_foreign_parsed(&["SV-123_rule"]);
        parsed.benchmark.as_mut().unwrap().publisher = Some("DISA".into());
        let rule = &mut parsed.rules[0];
        rule.severity = Some("high".into());
        rule.rationale = Some("The source rationale.".into());
        rule.identifiers = vec![
            crate::compliance::xccdf::models::StandardIdentifier {
                system: "https://public.cyber.mil/stigs/srg".into(),
                value: "SRG-OS-000001".into(),
            },
            crate::compliance::xccdf::models::StandardIdentifier {
                system: "https://public.cyber.mil/stigs/cci".into(),
                value: "CCI-000001".into(),
            },
        ];

        let validated = validate_import_plan(valid_plan(&["SV-123_rule"]), &parsed)
            .expect("DISA STIG import plan is valid");
        let records = build_policy_records(&validated);
        let metadata = &records[0].compliance_metadata;

        assert_eq!(metadata["category"], "security");
        assert_eq!(metadata["framework"], "DISA STIG");
        assert_eq!(metadata["severity"], "high");
        assert_eq!(metadata["rationale"], "The source rationale.");
        assert_eq!(metadata["srg_ids"], serde_json::json!(["SRG-OS-000001"]));
        assert_eq!(metadata["cci_ids"], serde_json::json!(["CCI-000001"]));
        assert!(metadata.get("control_family").is_none());
        assert!(metadata.get("cmmc_level").is_none());
        assert!(metadata.get("cis_section").is_none());
    }

    #[test]
    fn foreign_rule_without_source_severity_or_rationale_does_not_invent_them() {
        let mut parsed = minimal_foreign_parsed(&["rule-1"]);
        parsed.rules[0].severity = None;
        parsed.rules[0].rationale = None;

        let validated = validate_import_plan(valid_plan(&["rule-1"]), &parsed)
            .expect("foreign import plan is valid");
        let records = build_policy_records(&validated);
        let metadata = &records[0].compliance_metadata;

        assert!(metadata.get("severity").is_none());
        assert!(metadata.get("rationale").is_none());
        assert!(metadata.get("category").is_none());
        assert!(metadata.get("framework").is_none());
    }

    #[test]
    fn native_custom_check_requires_config_binding_and_preserves_assertions() {
        let parsed = minimal_foreign_parsed(&["rule-1"]);
        let mut plan = valid_plan(&["rule-1"]);
        plan.rule_actions = vec![XccdfRuleImportAction::CreateNativeCustom {
            rule_id: "rule-1".into(),
            customization: Default::default(),
            custom_check: ImportedCustomCheck {
                mode: "any".into(),
                rules: vec![ImportedCustomCheckRule {
                    field_name: "firewallEnabled".into(),
                    expression: "config.networking.firewall.enable".into(),
                    description: "Firewall is enabled".into(),
                    strict: false,
                }],
            },
            evidence_requirements: Vec::new(),
        }];

        let validated = validate_import_plan(plan, &parsed).expect("native plan is valid");
        let records = build_policy_records(&validated);
        assert_eq!(records[0].implementation_state, "native");
        assert_eq!(records[0].policy_type, "custom_check");
        assert_eq!(records[0].config["mode"], "any");
        assert_eq!(
            records[0].config["rules"][0]["field_name"],
            "firewallEnabled"
        );
        assert_eq!(records[0].config["rules"][0]["strict"], false);
    }

    #[test]
    fn native_custom_check_rejects_non_cfg_expression() {
        let parsed = minimal_foreign_parsed(&["rule-1"]);
        let mut plan = valid_plan(&["rule-1"]);
        plan.rule_actions = vec![XccdfRuleImportAction::CreateNativeCustom {
            rule_id: "rule-1".into(),
            customization: Default::default(),
            custom_check: ImportedCustomCheck {
                mode: "all".into(),
                rules: vec![ImportedCustomCheckRule {
                    field_name: "bad".into(),
                    expression: "true".into(),
                    description: "Not bound to the evaluated config".into(),
                    strict: true,
                }],
            },
            evidence_requirements: Vec::new(),
        }];

        let result = validate_import_plan(plan, &parsed);
        assert!(matches!(
            result,
            Err(ImportPlanError {
                code: "IMPORT_NATIVE_CHECK_INVALID",
                ..
            })
        ));
    }

    #[test]
    fn explicit_existing_mapping_remains_a_non_generated_import_action() {
        let parsed = minimal_foreign_parsed(&["rule-1"]);
        let mapped_version_id = Uuid::new_v4();
        let mut plan = valid_plan(&["rule-1"]);
        plan.rule_actions = vec![XccdfRuleImportAction::MapExisting {
            rule_id: "rule-1".into(),
            policy_version_id: mapped_version_id,
            proof: None,
        }];

        let validated = validate_import_plan(plan, &parsed).expect("mapping plan is valid");
        let records = build_policy_records(&validated);
        assert_eq!(records[0].mapped_policy_version_id, Some(mapped_version_id));
        assert_eq!(records[0].implementation_state, "mapped");
        assert_eq!(records[0].policy_type, "imported_xccdf");
    }

    #[test]
    fn deterministic_proof_cannot_be_combined_with_related_review() {
        let parsed = minimal_foreign_parsed(&["rule-1"]);
        let policy_version_id = Uuid::new_v4();
        let related = crate::compliance::xccdf::import_models::ReviewedRelatedCandidate {
            policy_version_id,
            related_requirement_version_id: Uuid::new_v4(),
            shared_cci_ids: vec!["CCI-000001".into()],
            shared_srg_ids: vec![],
        };
        for proof in [
            Some(MapExistingProof::ExactTechnicalMatch),
            Some(MapExistingProof::InheritedMapping),
        ] {
            let mut plan = valid_plan(&["rule-1"]);
            plan.rule_actions = vec![XccdfRuleImportAction::MapExisting {
                rule_id: "rule-1".into(),
                policy_version_id,
                proof,
            }];
            plan.mapping_semantics.insert(
                "rule-1".into(),
                ImportedMappingSemantics {
                    reviewed_related_candidate: Some(related.clone()),
                    ..Default::default()
                },
            );
            let result = validate_import_plan(plan, &parsed);
            assert!(matches!(
                result,
                Err(ImportPlanError {
                    code: "IMPORT_RELATED_REVIEW_INVALID",
                    ..
                })
            ));
        }
    }

    #[test]
    fn reviewed_related_requires_explicit_mapping_semantics() {
        let parsed = minimal_foreign_parsed(&["rule-1"]);
        let policy_version_id = Uuid::new_v4();
        let related = crate::compliance::xccdf::import_models::ReviewedRelatedCandidate {
            policy_version_id,
            related_requirement_version_id: Uuid::new_v4(),
            shared_cci_ids: vec!["CCI-000001".into()],
            shared_srg_ids: vec![],
        };
        let mut plan = valid_plan(&["rule-1"]);
        plan.rule_actions = vec![XccdfRuleImportAction::MapExisting {
            rule_id: "rule-1".into(),
            policy_version_id,
            proof: None,
        }];
        plan.mapping_semantics.insert(
            "rule-1".into(),
            ImportedMappingSemantics {
                reviewed_related_candidate: Some(related),
                ..Default::default()
            },
        );
        assert!(matches!(
            validate_import_plan(plan, &parsed),
            Err(ImportPlanError {
                code: "IMPORT_RELATED_REVIEW_INVALID",
                ..
            })
        ));
    }

    #[test]
    fn exact_technical_reuse_requires_explicit_mapping_semantics() {
        let parsed = minimal_foreign_parsed(&["rule-1"]);
        let policy_version_id = Uuid::new_v4();
        let mut plan = valid_plan(&["rule-1"]);
        plan.rule_actions = vec![XccdfRuleImportAction::MapExisting {
            rule_id: "rule-1".into(),
            policy_version_id,
            proof: Some(MapExistingProof::ExactTechnicalMatch),
        }];
        assert!(matches!(
            validate_import_plan(plan, &parsed),
            Err(ImportPlanError {
                code: "IMPORT_MAPPING_SEMANTICS_INVALID",
                ..
            })
        ));
    }

    #[test]
    fn rule_customization_overrides_local_policy_fields() {
        let parsed = minimal_foreign_parsed(&["rule-1"]);
        let mut plan = valid_plan(&["rule-1"]);
        plan.rule_actions = vec![XccdfRuleImportAction::CreateManual {
            rule_id: "rule-1".into(),
            customization: ImportedPolicyCustomization {
                policy_name: Some("Local firewall policy".into()),
                policy_description: Some("Local control description".into()),
                implementation_note: Some("Reviewed by platform security".into()),
                policy_severity: Some("high".into()),
                policy_rationale: Some("Apply the approved remediation".into()),
            },
            evidence_requirements: Vec::new(),
        }];

        let validated = validate_import_plan(plan, &parsed).expect("customized plan is valid");
        let records = build_policy_records(&validated);
        assert_eq!(records[0].name, "Local firewall policy");
        assert_eq!(
            records[0].description.as_deref(),
            Some("Local control description")
        );
        assert_eq!(
            records[0].compliance_metadata["implementation_note"],
            "Reviewed by platform security"
        );
        assert_eq!(records[0].compliance_metadata["policy_severity"], "high");
        assert_eq!(
            records[0].compliance_metadata["policy_rationale"],
            "Apply the approved remediation"
        );
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
            customization: Default::default(),
            evidence_requirements: Vec::new(),
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
                customization: Default::default(),
                evidence_requirements: Vec::new(),
            },
            XccdfRuleImportAction::CreateUnbound {
                rule_id: "r1".into(),
                customization: Default::default(),
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
            customization: Default::default(),
            evidence_requirements: Vec::new(),
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
            customization: Default::default(),
            evidence_requirements: Vec::new(),
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
    fn malformed_cf_native_document_is_rejected_as_invalid_metadata() {
        let mut parsed = minimal_foreign_parsed(&["r1"]);
        parsed.class = DocumentClass::CfNativeExact;
        let err = check_document_class(&parsed);
        assert!(err.is_some());
        assert_eq!(err.unwrap().code, "CF_NATIVE_METADATA_INVALID");
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
                    customization: Default::default(),
                    evidence_requirements: Vec::new(),
                },
                XccdfRuleImportAction::Exclude {
                    rule_id: "r2".into(),
                },
            ],
            mapping_semantics: std::collections::HashMap::new(),

            shared_group_decisions: Vec::new(),

            bundle: ImportedBundlePlan {
                name: "Bundle".into(),
                framework: "FW".into(),
                version: "1.0".into(),
                layer: None,
                owner: None,
                description: None,
                environment_ids: Vec::new(),
            },
        };
        let validated = validate_import_plan(plan, &parsed).unwrap();
        let records = build_policy_records(&validated);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].source_rule_id, "r1");
        assert_eq!(records[0].implementation_state, "manual");
    }

    #[test]
    fn duplicate_foreign_rule_titles_remain_source_faithful_before_commit() {
        let mut parsed = minimal_foreign_parsed(&["r1", "r2"]);
        parsed.rules[0].title = Some("Repeated title".into());
        parsed.rules[1].title = Some("Repeated title".into());
        let validated = validate_import_plan(valid_plan(&["r1", "r2"]), &parsed).unwrap();

        let records = build_policy_records(&validated);

        assert_eq!(records[0].name, "Repeated title");
        assert_eq!(records[1].name, "Repeated title");
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
