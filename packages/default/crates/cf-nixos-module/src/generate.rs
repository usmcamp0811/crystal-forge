//! Compose the generated module set from a resolved selection.
//!
//! This layer performs cross-policy conflict validation, builds the module
//! model, renders the Nix files, and produces the manifest. It performs no
//! file I/O, so the server or web UI can reuse it without shelling out.

use std::collections::BTreeMap;

use crate::model::{GeneratedPolicy, SkippedPolicy};
use crate::nix::{
    render_default_module, render_policy_module, render_single_file_module, safe_file_stem,
    safe_file_stem_full,
};
use crate::select::Selection;
use crate::{extract::extract_assignments, model::ResolvedPolicy};

/// Name of the machine-readable manifest emitted alongside the modules.
pub const MANIFEST_FILE: &str = "manifest.json";
/// Manifest schema version, independent of the generator version.
pub const MANIFEST_FORMAT_VERSION: &str = "1";
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
    /// Output-directory-relative path. Always forward-slash separated.
    pub path: String,
    pub contents: String,
}

/// The complete result of a generation run.
#[derive(Debug, Clone)]
pub struct Generated {
    pub files: Vec<GeneratedFile>,
    pub implemented: Vec<GeneratedPolicy>,
    pub skipped: Vec<SkippedPolicy>,
}

/// Generation output layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    /// `default.nix` plus one file per policy under `policies/`.
    Directory,
    /// A single combined `default.nix`.
    SingleFile,
}

/// Build the module set for a selection.
///
/// Returns `Err` only for conflicting NixOS implementations, which are never
/// resolved automatically. Policies that cannot be converted are returned in
/// `skipped` and are never represented as implemented.
pub fn generate(selection: &Selection, layout: Layout) -> Result<Generated, Vec<OptionConflict>> {
    let mut implemented: Vec<GeneratedPolicy> = Vec::new();
    let mut skipped: Vec<SkippedPolicy> = Vec::new();

    for policy in &selection.policies {
        match extract_assignments(policy) {
            Ok(assignments) => implemented.push(GeneratedPolicy {
                policy: policy.clone(),
                assignments,
                // Filled in below, once the layout is known.
                generated_file: String::new(),
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

    assign_file_names(&mut implemented);

    let mut files = Vec::new();
    match layout {
        Layout::Directory => {
            files.push(GeneratedFile {
                path: "default.nix".to_string(),
                contents: render_default_module(&implemented),
            });
            for generated in &implemented {
                files.push(GeneratedFile {
                    path: generated.generated_file.clone(),
                    contents: render_policy_module(generated),
                });
            }
        }
        Layout::SingleFile => {
            files.push(GeneratedFile {
                path: "default.nix".to_string(),
                contents: render_single_file_module(&implemented),
            });
        }
    }

    files.push(GeneratedFile {
        path: MANIFEST_FILE.to_string(),
        contents: render_manifest(selection, &implemented, &skipped, layout),
    });

    files.sort_by(|a, b| a.path.cmp(&b.path));

    Ok(Generated {
        files,
        implemented,
        skipped,
    })
}

/// Assign a deterministic, unique file name to every implemented policy.
///
/// The short form uses a truncated version UUID for readability. If two
/// policies would collide, every colliding entry falls back to the full version
/// UUID, which is unique by construction. `implemented` is already ordered
/// deterministically, so the result is stable across runs.
fn assign_file_names(implemented: &mut [GeneratedPolicy]) {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for generated in implemented.iter() {
        let stem = safe_file_stem(&generated.policy.name, generated.policy.policy_version_id);
        *counts.entry(stem).or_insert(0) += 1;
    }

    for generated in implemented.iter_mut() {
        let short = safe_file_stem(&generated.policy.name, generated.policy.policy_version_id);
        let stem = if counts.get(&short).copied().unwrap_or(0) > 1 {
            safe_file_stem_full(&generated.policy.name, generated.policy.policy_version_id)
        } else {
            short
        };
        generated.generated_file = format!("policies/{stem}.nix");
    }
}

/// Detect policies that assign the same NixOS option different values.
fn detect_conflicts(implemented: &[GeneratedPolicy]) -> Vec<OptionConflict> {
    let mut owners: BTreeMap<&str, (&ResolvedPolicy, String)> = BTreeMap::new();
    let mut conflicts = Vec::new();

    for generated in implemented {
        for assignment in &generated.assignments {
            let value = assignment.value.nix_repr();
            match owners.get(assignment.option_path.as_str()) {
                Some((existing_policy, existing_value)) if *existing_value != value => {
                    conflicts.push(OptionConflict {
                        option_path: assignment.option_path.clone(),
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
                    owners.insert(assignment.option_path.as_str(), (&generated.policy, value));
                }
            }
        }
    }

    conflicts.sort_by(|a, b| a.option_path.cmp(&b.option_path));
    conflicts
}

/// Build the machine-readable manifest.
///
/// Serialized with sorted keys and a trailing newline so identical inputs
/// produce byte-identical output that is safe to commit.
fn render_manifest(
    selection: &Selection,
    implemented: &[GeneratedPolicy],
    skipped: &[SkippedPolicy],
    layout: Layout,
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
                "generated_file": generated.generated_file,
                "source_input": generated.policy.origin.input_label,
                "source_export_digest": generated.policy.origin.source_sha256,
                "bundle_version_id": generated
                    .policy
                    .origin
                    .bundle_version_id
                    .map(|id| id.to_string()),
                "compliance_mappings": generated.policy.compliance_mappings(),
                "nixos_options": generated
                    .assignments
                    .iter()
                    .map(|assignment| {
                        serde_json::json!({
                            "option_path": assignment.option_path,
                            "value": assignment.value.nix_repr(),
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
                "source_input": entry.policy.origin.input_label,
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
                "source_input": bundle.input_label,
                "source_export_digest": bundle.source_sha256,
                "selected_policy_version_ids": bundle
                    .selected_policy_version_ids
                    .iter()
                    .map(|id| id.to_string())
                    .collect::<Vec<_>>(),
            })
        })
        .collect();

    let manifest = serde_json::json!({
        "format_version": MANIFEST_FORMAT_VERSION,
        "generator": GENERATOR_NAME,
        "layout": match layout {
            Layout::Directory => "directory",
            Layout::SingleFile => "single-file",
        },
        "bundles": bundles,
        "policies": policies,
        "skipped_policies": skipped_policies,
    });

    // `serde_json::Value` uses a BTreeMap, so keys serialize in sorted order.
    let mut text = serde_json::to_string_pretty(&manifest).unwrap_or_else(|_| "{}".to_string());
    text.push('\n');
    text
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::*;
    use crate::model::PolicyOrigin;

    fn policy(name: &str, version_id: &str, expression: &str) -> ResolvedPolicy {
        ResolvedPolicy {
            policy_id: Uuid::parse_str("11111111-1111-1111-1111-111111111111").expect("uuid"),
            policy_version_id: Uuid::parse_str(version_id).expect("uuid"),
            version: "1".into(),
            name: name.into(),
            description: None,
            policy_type: "custom_check".into(),
            implementation_state: "native".into(),
            execution_phase: "nix-evaluation".into(),
            config: serde_json::json!({"expression": expression}),
            compliance_metadata: serde_json::json!({}),
            semantic_digest: format!("digest-{name}"),
            origin: PolicyOrigin {
                input_label: "in.json".into(),
                source_sha256: "f00d".into(),
                bundle_version_id: None,
            },
        }
    }

    fn manual(name: &str, version_id: &str) -> ResolvedPolicy {
        let mut p = policy(name, version_id, "config.a.b == true");
        p.implementation_state = "manual".into();
        p
    }

    const V1: &str = "22222222-2222-2222-2222-222222222222";
    const V2: &str = "33333333-3333-3333-3333-333333333333";

    fn selection(policies: Vec<ResolvedPolicy>) -> Selection {
        Selection {
            policies,
            bundles: Vec::new(),
            deduplicated: Vec::new(),
        }
    }

    #[test]
    fn generates_a_module_per_policy_plus_default_and_manifest() {
        let sel = selection(vec![
            policy("alpha", V1, "config.networking.firewall.enable == true"),
            policy("beta", V2, "config.services.openssh.enable == true"),
        ]);
        let generated = generate(&sel, Layout::Directory).expect("no conflict");
        let paths: Vec<_> = generated.files.iter().map(|f| f.path.as_str()).collect();
        assert!(paths.contains(&"default.nix"));
        assert!(paths.contains(&"manifest.json"));
        assert_eq!(generated.implemented.len(), 2);
        assert_eq!(paths.len(), 4);
    }

    #[test]
    fn conflicting_option_values_are_reported_not_resolved() {
        let sel = selection(vec![
            policy("alpha", V1, "config.services.openssh.enable == true"),
            policy("beta", V2, "config.services.openssh.enable == false"),
        ]);
        let conflicts = generate(&sel, Layout::Directory).expect_err("must conflict");
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].option_path, "services.openssh.enable");
        let rendered = conflicts[0].to_string();
        assert!(rendered.contains("alpha"), "{rendered}");
        assert!(rendered.contains("beta"), "{rendered}");
        assert!(rendered.contains("true"), "{rendered}");
        assert!(rendered.contains("false"), "{rendered}");
    }

    #[test]
    fn agreeing_policies_are_not_a_conflict() {
        let sel = selection(vec![
            policy("alpha", V1, "config.services.openssh.enable == true"),
            policy("beta", V2, "config.services.openssh.enable == true"),
        ]);
        generate(&sel, Layout::Directory).expect("agreement is not a conflict");
    }

    #[test]
    fn unsupported_policies_are_skipped_and_never_implemented() {
        let sel = selection(vec![
            policy("alpha", V1, "config.a.b == true"),
            manual("needs-human", V2),
        ]);
        let generated = generate(&sel, Layout::Directory).expect("ok");
        assert_eq!(generated.implemented.len(), 1);
        assert_eq!(generated.skipped.len(), 1);
        assert_eq!(generated.skipped[0].policy.name, "needs-human");

        // The skipped policy must not appear as an import or an assignment.
        let default_nix = generated
            .files
            .iter()
            .find(|f| f.path == "default.nix")
            .expect("default.nix");
        assert!(!default_nix.contents.contains("needs-human"));
    }

    #[test]
    fn manifest_records_identities_and_skips() {
        let sel = selection(vec![
            policy("alpha", V1, "config.a.b == true"),
            manual("needs-human", V2),
        ]);
        let generated = generate(&sel, Layout::Directory).expect("ok");
        let manifest = generated
            .files
            .iter()
            .find(|f| f.path == "manifest.json")
            .expect("manifest");
        let parsed: serde_json::Value =
            serde_json::from_str(&manifest.contents).expect("valid JSON");

        assert_eq!(parsed["format_version"], "1");
        assert!(
            parsed["generator"]
                .as_str()
                .expect("generator")
                .starts_with("cf-nixos-module ")
        );
        assert_eq!(parsed["policies"].as_array().expect("policies").len(), 1);
        assert_eq!(parsed["policies"][0]["policy_version_id"], V1);
        assert_eq!(parsed["policies"][0]["semantic_digest"], "digest-alpha");
        assert_eq!(
            parsed["policies"][0]["generated_file"],
            "policies/alpha-22222222.nix"
        );
        assert_eq!(parsed["skipped_policies"][0]["reason_code"], "not_native");
        assert_eq!(parsed["skipped_policies"][0]["policy_version_id"], V2);
    }

    #[test]
    fn generation_is_byte_for_byte_deterministic() {
        let sel = selection(vec![
            policy("alpha", V1, "config.a.b == true"),
            policy("beta", V2, "config.c.d == false"),
        ]);
        let first = generate(&sel, Layout::Directory).expect("ok");
        let second = generate(&sel, Layout::Directory).expect("ok");
        assert_eq!(first.files, second.files);
    }

    #[test]
    fn single_file_layout_emits_one_module() {
        let sel = selection(vec![policy("alpha", V1, "config.a.b == true")]);
        let generated = generate(&sel, Layout::SingleFile).expect("ok");
        let paths: Vec<_> = generated.files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, vec!["default.nix", "manifest.json"]);
        let default_nix = &generated.files[0].contents;
        assert!(default_nix.contains("a.b = true;"));
    }

    #[test]
    fn colliding_file_names_fall_back_to_the_full_version_uuid() {
        // Same name, and version UUIDs sharing their first 8 hex characters.
        let mut a = policy(
            "same-name",
            "22222222-0000-0000-0000-000000000001",
            "config.a.b == true",
        );
        let mut b = policy(
            "same-name",
            "22222222-0000-0000-0000-000000000002",
            "config.c.d == true",
        );
        a.semantic_digest = "d1".into();
        b.semantic_digest = "d2".into();

        let generated = generate(&selection(vec![a, b]), Layout::Directory).expect("ok");
        let files: Vec<_> = generated
            .implemented
            .iter()
            .map(|g| g.generated_file.clone())
            .collect();
        assert_eq!(files.len(), 2);
        assert_ne!(files[0], files[1], "file names must be unique");
        assert!(
            files[0].contains("222222220000000000000000000000"),
            "{}",
            files[0]
        );
    }

    #[test]
    fn generated_paths_never_escape_the_output_directory() {
        let sel = selection(vec![policy("../../etc/passwd", V1, "config.a.b == true")]);
        let generated = generate(&sel, Layout::Directory).expect("ok");
        for file in &generated.files {
            assert!(!file.path.contains(".."), "{}", file.path);
            assert!(!file.path.starts_with('/'), "{}", file.path);
        }
    }

    #[test]
    fn an_empty_selection_still_produces_an_importable_module() {
        let generated = generate(&selection(Vec::new()), Layout::Directory).expect("ok");
        let default_nix = generated
            .files
            .iter()
            .find(|f| f.path == "default.nix")
            .expect("default.nix");
        assert!(default_nix.contents.contains("imports = ["));
    }
}
