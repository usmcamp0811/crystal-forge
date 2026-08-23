//! Compose the generated artifact from a resolved selection.
//!
//! This layer performs cross-policy conflict validation, normalizes the
//! supported NixOS assignments, and renders the artifact. It performs no file
//! I/O, so the server or web UI can reuse it without shelling out.
//!
//! `manifest.json` is the canonical generated representation. It serves both
//! audit/provenance and module construction, and it contains only data:
//! assignments are `path` component lists with typed JSON values, never Nix
//! source strings.

use std::collections::BTreeMap;

use uuid::Uuid;

use crate::extract::extract_assignments;
use crate::model::{GeneratedPolicy, ResolvedPolicy, SkippedPolicy};
use crate::nix::{COMPLIANCE_LIB_NIX, LIB_FILE, render_default_module, sanitize_baseline_name};
use crate::select::Selection;

/// Name of the machine-readable manifest emitted alongside the module.
pub const MANIFEST_FILE: &str = "manifest.json";
/// Manifest schema version, independent of the generator version.
pub const MANIFEST_FORMAT_VERSION: &str = "2";
/// Generator identity recorded in the manifest.
pub const GENERATOR_NAME: &str = concat!("cf-nixos-module ", env!("CARGO_PKG_VERSION"));

/// Two policies configure the same NixOS option differently.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OptionConflict {
    pub option_path: String,
    pub first_policy: String,
    pub first_version_id: String,
    pub first_value: String,
    pub second_policy: String,
    pub second_version_id: String,
    pub second_value: String,
}

impl std::fmt::Display for OptionConflict {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "option {} is set to {} by '{}' ({}) and to {} by '{}' ({})",
            self.option_path,
            self.first_value,
            self.first_policy,
            self.first_version_id,
            self.second_value,
            self.second_policy,
            self.second_version_id
        )
    }
}

/// One generated file and its contents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratedFile {
    /// Output-directory-relative path.
    pub path: String,
    pub contents: String,
}

/// The complete result of a generation run.
#[derive(Debug, Clone)]
pub struct Generated {
    pub files: Vec<GeneratedFile>,
    pub implemented: Vec<GeneratedPolicy>,
    pub skipped: Vec<SkippedPolicy>,
    /// The Nix-safe baseline identifier the consumer enables.
    pub baseline: String,
}

/// Choose the baseline identifier for a selection.
///
/// Derivation is deterministic and independent of CLI argument order:
///
/// 1. an explicit `--baseline` name, when given;
/// 2. otherwise the single contributing bundle's name plus immutable version ID;
/// 3. otherwise the lexicographically first input label.
pub fn derive_baseline(selection: &Selection, explicit: Option<&str>) -> String {
    if let Some(name) = explicit {
        return sanitize_baseline_name(name);
    }

    if selection.bundles.len() == 1 {
        let bundle = &selection.bundles[0];
        return sanitize_baseline_with_identity(&bundle.name, bundle.bundle_version_id);
    }

    let mut labels: Vec<&str> = selection
        .policies
        .iter()
        .flat_map(|policy| policy.origin.input_labels.iter().map(String::as_str))
        .chain(
            selection
                .bundles
                .iter()
                .flat_map(|bundle| bundle.input_labels.iter().map(String::as_str)),
        )
        .collect();
    labels.sort_unstable();

    labels
        .first()
        .map(|label| sanitize_baseline_name(strip_extension(label)))
        .unwrap_or_else(|| "baseline".to_string())
}

/// Preserve the immutable bundle-version suffix even when the display name is
/// long enough to exceed the Nix identifier limit.
fn sanitize_baseline_with_identity(name: &str, identity: Uuid) -> String {
    let suffix = format!("-{}", identity.simple());
    let mut prefix = sanitize_baseline_name(name);
    let prefix_limit = 64usize.saturating_sub(suffix.len());
    prefix.truncate(prefix_limit);
    while prefix.ends_with('-') {
        prefix.pop();
    }
    if prefix.is_empty() {
        prefix.push_str("baseline");
    }
    sanitize_baseline_name(&format!("{prefix}{suffix}"))
}

fn strip_extension(label: &str) -> &str {
    match label.rsplit_once('.') {
        Some((stem, _)) if !stem.is_empty() => stem,
        _ => label,
    }
}

/// Build the artifact for a selection.
///
/// Returns `Err` only for conflicting NixOS implementations, which are never
/// resolved automatically. Policies that cannot be converted are returned in
/// `skipped` and are never represented as implemented.
pub fn generate(selection: &Selection, baseline: &str) -> Result<Generated, Vec<OptionConflict>> {
    let mut implemented: Vec<GeneratedPolicy> = Vec::new();
    let mut skipped: Vec<SkippedPolicy> = Vec::new();

    for policy in &selection.policies {
        match extract_assignments(policy) {
            Ok(assignments) => implemented.push(GeneratedPolicy {
                policy: policy.clone(),
                assignments,
            }),
            Err(reason) => skipped.push(SkippedPolicy {
                policy: policy.clone(),
                reason,
            }),
        }
    }

    let conflicts = detect_conflicts(&implemented);
    if !conflicts.is_empty() {
        return Err(conflicts);
    }

    let files = vec![
        GeneratedFile {
            path: "default.nix".to_string(),
            contents: render_default_module(baseline, implemented.len(), skipped.len()),
        },
        GeneratedFile {
            path: LIB_FILE.to_string(),
            contents: COMPLIANCE_LIB_NIX.to_string(),
        },
        GeneratedFile {
            path: MANIFEST_FILE.to_string(),
            contents: render_manifest(selection, baseline, &implemented, &skipped),
        },
    ];

    Ok(Generated {
        files,
        implemented,
        skipped,
        baseline: baseline.to_string(),
    })
}

/// Detect policies that assign the same NixOS option different values.
fn detect_conflicts(implemented: &[GeneratedPolicy]) -> Vec<OptionConflict> {
    let mut owners: BTreeMap<String, (&ResolvedPolicy, String)> = BTreeMap::new();
    let mut conflicts = Vec::new();

    for generated in implemented {
        for assignment in &generated.assignments {
            let path = assignment.dotted_path();
            let value = assignment.value_display();
            match owners.get(&path) {
                Some((existing_policy, existing_value)) if *existing_value != value => {
                    conflicts.push(OptionConflict {
                        option_path: path.clone(),
                        first_policy: existing_policy.name.clone(),
                        first_version_id: existing_policy.policy_version_id.to_string(),
                        first_value: existing_value.clone(),
                        second_policy: generated.policy.name.clone(),
                        second_version_id: generated.policy.policy_version_id.to_string(),
                        second_value: value,
                    });
                }
                Some(_) => {
                    // Two policies agreeing on the same value is not a conflict.
                }
                None => {
                    owners.insert(path, (&generated.policy, value));
                }
            }
        }
    }

    conflicts.sort_by(|a, b| a.option_path.cmp(&b.option_path));
    conflicts
}

/// Build the canonical manifest.
///
/// `serde_json::Value` uses a `BTreeMap`, so keys serialize in sorted order and
/// identical inputs produce byte-identical output.
fn render_manifest(
    selection: &Selection,
    baseline: &str,
    implemented: &[GeneratedPolicy],
    skipped: &[SkippedPolicy],
) -> String {
    let policies: Vec<serde_json::Value> = implemented
        .iter()
        .map(|generated| {
            serde_json::json!({
                "policy_id": generated.policy.policy_id.to_string(),
                "policy_version_id": generated.policy.policy_version_id.to_string(),
                "name": generated.policy.name,
                "version": generated.policy.version,
                "policy_type": generated.policy.policy_type,
                "implementation_state": generated.policy.implementation_state,
                "semantic_digest": generated.policy.semantic_digest,
                "generated_file": MANIFEST_FILE,
                "source_inputs": generated.policy.origin.input_labels,
                "source_export_digests": generated.policy.origin.source_sha256s,
                "bundle_version_ids": generated
                    .policy
                    .origin
                    .bundle_version_ids
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>(),
                // Mappings are metadata. One policy may map to DISA, NIST, CIS,
                // and CMMC requirements while keeping one implementation.
                "compliance_mappings": generated.policy.compliance_mappings(),
                // Normalized, data-only NixOS assignments.
                "assignments": generated
                    .assignments
                    .iter()
                    .map(|assignment| {
                        serde_json::json!({
                            "path": assignment.path,
                            "value": assignment.value,
                        })
                    })
                    .collect::<Vec<_>>(),
            })
        })
        .collect();

    let skipped_policies: Vec<serde_json::Value> = skipped
        .iter()
        .map(|entry| {
            serde_json::json!({
                "policy_id": entry.policy.policy_id.to_string(),
                "policy_version_id": entry.policy.policy_version_id.to_string(),
                "name": entry.policy.name,
                "policy_type": entry.policy.policy_type,
                "implementation_state": entry.policy.implementation_state,
                "semantic_digest": entry.policy.semantic_digest,
                "reason_code": entry.reason.code(),
                "reason": entry.reason.to_string(),
                "source_inputs": entry.policy.origin.input_labels,
            })
        })
        .collect();

    let bundles: Vec<serde_json::Value> = selection
        .bundles
        .iter()
        .map(|bundle| {
            serde_json::json!({
                "bundle_id": bundle.bundle_id.to_string(),
                "bundle_version_id": bundle.bundle_version_id.to_string(),
                "name": bundle.name,
                "version": bundle.version,
                "framework": bundle.framework,
                "framework_version": bundle.framework_version,
                "publication_state": bundle.publication_state,
                "semantic_digest": bundle.semantic_digest,
                "source_inputs": bundle.input_labels,
                "source_export_digests": bundle.source_sha256s,
                "selected_policy_version_ids": bundle
                    .selected_policy_version_ids
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>(),
            })
        })
        .collect();

    let manifest = serde_json::json!({
        "format_version": MANIFEST_FORMAT_VERSION,
        "generator": GENERATOR_NAME,
        "baseline": baseline,
        "bundles": bundles,
        "policies": policies,
        "skipped_policies": skipped_policies,
    });

    let mut text = serde_json::to_string_pretty(&manifest).unwrap_or_else(|_| "{}".to_string());
    text.push('\n');
    text
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::*;
    use crate::model::PolicyOrigin;

    fn policy(name: &str, lineage: &str, version_id: &str, expression: &str) -> ResolvedPolicy {
        ResolvedPolicy {
            policy_id: Uuid::parse_str(lineage).expect("uuid"),
            policy_version_id: Uuid::parse_str(version_id).expect("uuid"),
            version: "1".into(),
            publication_state: "accepted".into(),
            name: name.into(),
            description: None,
            policy_type: "custom_check".into(),
            implementation_state: "native".into(),
            execution_phase: "nix-evaluation".into(),
            config: serde_json::json!({"expression": expression}),
            compliance_metadata: serde_json::json!({}),
            semantic_digest: format!("digest-{name}"),
            origin: PolicyOrigin::new("in.json".into(), "f00d".into(), None),
        }
    }

    fn manual(name: &str, lineage: &str, version_id: &str) -> ResolvedPolicy {
        let mut p = policy(name, lineage, version_id, "config.a.b == true");
        p.implementation_state = "manual".into();
        p
    }

    const L1: &str = "11111111-1111-1111-1111-111111111111";
    const L2: &str = "11111111-2222-2222-2222-222222222222";
    const V1: &str = "22222222-1111-1111-1111-111111111111";
    const V2: &str = "22222222-2222-2222-2222-222222222222";

    fn selection(policies: Vec<ResolvedPolicy>) -> Selection {
        Selection {
            policies,
            bundles: Vec::new(),
            deduplicated: Vec::new(),
        }
    }

    fn bundle(version_id: &str) -> crate::model::ResolvedBundle {
        crate::model::ResolvedBundle {
            bundle_id: Uuid::parse_str("aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa").expect("uuid"),
            bundle_version_id: Uuid::parse_str(version_id).expect("uuid"),
            name: "same named baseline".into(),
            version: "1".into(),
            framework: None,
            framework_version: None,
            publication_state: "accepted".into(),
            semantic_digest: None,
            source_sha256s: vec![],
            input_labels: vec!["bundle.xml".into()],
            selected_policy_version_ids: vec![],
        }
    }

    fn file<'a>(generated: &'a Generated, path: &str) -> &'a str {
        generated
            .files
            .iter()
            .find(|f| f.path == path)
            .unwrap_or_else(|| panic!("missing {path}"))
            .contents
            .as_str()
    }

    fn manifest_of(generated: &Generated) -> serde_json::Value {
        serde_json::from_str(file(generated, "manifest.json")).expect("valid JSON")
    }

    #[test]
    fn artifact_has_exactly_three_files() {
        let sel = selection(vec![
            policy("alpha", L1, V1, "config.networking.firewall.enable == true"),
            policy("beta", L2, V2, "config.services.openssh.enable == true"),
        ]);
        let generated = generate(&sel, "test").expect("no conflict");
        let mut paths: Vec<_> = generated.files.iter().map(|f| f.path.as_str()).collect();
        paths.sort_unstable();
        assert_eq!(paths, vec!["default.nix", "lib.nix", "manifest.json"]);
    }

    #[test]
    fn assignments_are_typed_json_not_nix_source() {
        let sel = selection(vec![
            policy("b", L1, V1, "config.services.openssh.enable == true"),
            policy(
                "s",
                L2,
                V2,
                "config.services.openssh.settings.PermitRootLogin == \"no\"",
            ),
        ]);
        let manifest = manifest_of(&generate(&sel, "test").expect("ok"));
        let policies = manifest["policies"].as_array().expect("policies");

        let boolean = &policies[0]["assignments"][0];
        assert_eq!(
            boolean["path"],
            serde_json::json!(["services", "openssh", "enable"])
        );
        assert_eq!(boolean["value"], serde_json::Value::Bool(true));
        assert!(
            boolean["value"].is_boolean(),
            "value must be a JSON boolean"
        );

        let string = &policies[1]["assignments"][0];
        assert_eq!(
            string["path"],
            serde_json::json!(["services", "openssh", "settings", "PermitRootLogin"])
        );
        assert_eq!(string["value"], serde_json::Value::String("no".into()));
    }

    #[test]
    fn integer_assignments_are_json_numbers() {
        let sel = selection(vec![policy("i", L1, V1, "config.services.x.port == 22")]);
        let manifest = manifest_of(&generate(&sel, "test").expect("ok"));
        let value = &manifest["policies"][0]["assignments"][0]["value"];
        assert!(value.is_i64(), "expected a JSON number, got {value}");
        assert_eq!(value, &serde_json::json!(22));
    }

    #[test]
    fn manifest_never_contains_a_nix_source_assignment_field() {
        let sel = selection(vec![policy("a", L1, V1, "config.a.b == true")]);
        let generated = generate(&sel, "test").expect("ok");
        let text = file(&generated, "manifest.json");
        assert!(
            !text.contains("option_path"),
            "legacy Nix-source field present"
        );
        assert!(!text.contains("nixos_options"));
    }

    #[test]
    fn conflicting_option_values_are_reported_not_resolved() {
        let sel = selection(vec![
            policy("alpha", L1, V1, "config.services.openssh.enable == true"),
            policy("beta", L2, V2, "config.services.openssh.enable == false"),
        ]);
        let conflicts = generate(&sel, "test").expect_err("must conflict");
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].option_path, "services.openssh.enable");
        let rendered = conflicts[0].to_string();
        assert!(
            rendered.contains("alpha") && rendered.contains("beta"),
            "{rendered}"
        );
    }

    #[test]
    fn agreeing_policies_are_not_a_conflict() {
        let sel = selection(vec![
            policy("alpha", L1, V1, "config.services.openssh.enable == true"),
            policy("beta", L2, V2, "config.services.openssh.enable == true"),
        ]);
        generate(&sel, "test").expect("agreement is not a conflict");
    }

    #[test]
    fn unsupported_policies_are_skipped_and_never_implemented() {
        let sel = selection(vec![
            policy("alpha", L1, V1, "config.a.b == true"),
            manual("needs-human", L2, V2),
        ]);
        let generated = generate(&sel, "test").expect("ok");
        assert_eq!(generated.implemented.len(), 1);
        assert_eq!(generated.skipped.len(), 1);

        // The skipped policy contributes no assignment anywhere.
        let manifest = manifest_of(&generated);
        assert_eq!(manifest["policies"].as_array().expect("policies").len(), 1);
        assert_eq!(manifest["skipped_policies"][0]["name"], "needs-human");
        assert_eq!(manifest["skipped_policies"][0]["reason_code"], "not_native");
    }

    #[test]
    fn manifest_records_full_provenance() {
        let sel = selection(vec![policy("alpha", L1, V1, "config.a.b == true")]);
        let manifest = manifest_of(&generate(&sel, "test").expect("ok"));

        assert_eq!(manifest["format_version"], "2");
        assert_eq!(manifest["baseline"], "test");
        let policy = &manifest["policies"][0];
        assert_eq!(policy["policy_id"], L1);
        assert_eq!(policy["policy_version_id"], V1);
        assert_eq!(policy["semantic_digest"], "digest-alpha");
        assert_eq!(policy["source_inputs"], serde_json::json!(["in.json"]));
        assert_eq!(policy["source_export_digests"], serde_json::json!(["f00d"]));
    }

    #[test]
    fn generation_is_byte_for_byte_deterministic() {
        let sel = selection(vec![
            policy("alpha", L1, V1, "config.a.b == true"),
            policy("beta", L2, V2, "config.c.d == false"),
        ]);
        assert_eq!(
            generate(&sel, "test").expect("ok").files,
            generate(&sel, "test").expect("ok").files
        );
    }

    #[test]
    fn an_empty_selection_still_produces_an_importable_module() {
        let generated = generate(&selection(Vec::new()), "test").expect("ok");
        assert!(file(&generated, "default.nix").contains("import ./lib.nix"));
        assert_eq!(
            manifest_of(&generated)["policies"]
                .as_array()
                .expect("policies")
                .len(),
            0
        );
    }

    #[test]
    fn baseline_derivation_prefers_an_explicit_name() {
        let sel = selection(Vec::new());
        assert_eq!(derive_baseline(&sel, Some("My Baseline")), "my-baseline");
    }

    #[test]
    fn baseline_derivation_is_independent_of_input_order() {
        let mut a = policy("alpha", L1, V1, "config.a.b == true");
        a.origin = PolicyOrigin::new("zulu.json".into(), "1".into(), None);
        let mut b = policy("beta", L2, V2, "config.c.d == true");
        b.origin = PolicyOrigin::new("alpha.json".into(), "2".into(), None);

        let forward = derive_baseline(&selection(vec![a.clone(), b.clone()]), None);
        let reverse = derive_baseline(&selection(vec![b, a]), None);
        assert_eq!(forward, reverse);
        assert_eq!(forward, "alpha");
    }

    #[test]
    fn default_baselines_for_bundle_versions_cannot_collide() {
        let first = Selection {
            policies: vec![],
            bundles: vec![bundle("bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb")],
            deduplicated: vec![],
        };
        let second = Selection {
            policies: vec![],
            bundles: vec![bundle("cccccccc-cccc-cccc-cccc-cccccccccccc")],
            deduplicated: vec![],
        };

        let first_name = derive_baseline(&first, None);
        let second_name = derive_baseline(&second, None);
        assert_ne!(first_name, second_name);
        assert!(first_name.contains("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"));
        assert!(second_name.contains("cccccccccccccccccccccccccccccccc"));
    }
}
