//! Integration tests: exported artifact -> generated compliance artifact.
//!
//! These drive the same layers the CLI uses, over real export documents built
//! by the fixture module (including real `cf-model-json-1` semantic digests).
//! Evaluation of the generated Nix through the NixOS module system is covered
//! by the `nixos-module-generation` flake check.

use cf_nixos_module::fixture::{bundle_xccdf_xml, policy_set_json};
use cf_nixos_module::generate::{Generated, derive_baseline, generate};
use cf_nixos_module::input::{LoadedInput, load_input};
use cf_nixos_module::select::{Selection, select_policies};

fn load(inputs: Vec<(&str, Vec<u8>)>) -> Vec<LoadedInput> {
    inputs
        .into_iter()
        .map(|(label, bytes)| load_input(&bytes, label).expect("input loads"))
        .collect()
}

fn resolve(inputs: Vec<(&str, Vec<u8>)>) -> Selection {
    select_policies(&load(inputs)).expect("no identity conflict")
}

fn run(inputs: Vec<(&str, Vec<u8>)>) -> Generated {
    let selection = resolve(inputs);
    let baseline = derive_baseline(&selection, Some("test"));
    generate(&selection, &baseline).expect("no option conflict")
}

fn policy_set_input() -> (&'static str, Vec<u8>) {
    ("policy-set.json", policy_set_json().into_bytes())
}

fn bundle_input() -> (&'static str, Vec<u8>) {
    ("bundle.xml", bundle_xccdf_xml().into_bytes())
}

fn file<'a>(generated: &'a Generated, path: &str) -> &'a str {
    generated
        .files
        .iter()
        .find(|f| f.path == path)
        .unwrap_or_else(|| panic!("missing generated file {path}"))
        .contents
        .as_str()
}

fn manifest(generated: &Generated) -> serde_json::Value {
    serde_json::from_str(file(generated, "manifest.json")).expect("valid JSON")
}

/// Locate a policy entry in the manifest by name.
fn manifest_policy(manifest: &serde_json::Value, name: &str) -> serde_json::Value {
    manifest["policies"]
        .as_array()
        .expect("policies")
        .iter()
        .find(|policy| policy["name"] == name)
        .unwrap_or_else(|| panic!("policy {name} not in manifest"))
        .clone()
}

#[test]
fn artifact_layout_is_minimal() {
    let generated = run(vec![policy_set_input(), bundle_input()]);
    let mut paths: Vec<_> = generated.files.iter().map(|f| f.path.as_str()).collect();
    paths.sort_unstable();
    assert_eq!(paths, vec!["default.nix", "lib.nix", "manifest.json"]);
}

#[test]
fn policy_set_export_produces_typed_assignments() {
    let generated = run(vec![policy_set_input()]);
    let manifest = manifest(&generated);

    let ssh = manifest_policy(&manifest, "disable-root-ssh-login");
    let assignments = ssh["assignments"].as_array().expect("assignments");
    assert_eq!(assignments.len(), 2);

    assert_eq!(
        assignments[0]["path"],
        serde_json::json!(["services", "openssh", "settings", "PasswordAuthentication"])
    );
    assert_eq!(assignments[0]["value"], serde_json::Value::Bool(false));

    assert_eq!(
        assignments[1]["path"],
        serde_json::json!(["services", "openssh", "settings", "PermitRootLogin"])
    );
    assert_eq!(
        assignments[1]["value"],
        serde_json::Value::String("no".into())
    );
}

#[test]
fn generated_nix_contains_no_policy_values() {
    // All policy data lives in manifest.json; the Nix is generic.
    let generated = run(vec![policy_set_input(), bundle_input()]);
    let default_nix = file(&generated, "default.nix");
    let lib_nix = file(&generated, "lib.nix");

    for leaked in [
        "PermitRootLogin",
        "networking.firewall",
        "timesyncd",
        "disable-root-ssh-login",
    ] {
        assert!(!default_nix.contains(leaked), "default.nix leaked {leaked}");
        assert!(!lib_nix.contains(leaked), "lib.nix leaked {leaked}");
    }
}

#[test]
fn generated_nix_never_evaluates_manifest_content() {
    let generated = run(vec![policy_set_input()]);
    let lib_nix = file(&generated, "lib.nix");

    // The library reads structured data; it must not evaluate strings.
    assert!(lib_nix.contains("setAttrByPath"));
    for forbidden in ["fromJSON", "import (", "builtins.exec", "toFile"] {
        assert!(
            !lib_nix.contains(forbidden),
            "lib.nix must not use {forbidden}"
        );
    }

    // default.nix decodes the manifest as data exactly once and never imports it.
    let default_nix = file(&generated, "default.nix");
    assert_eq!(default_nix.matches("fromJSON").count(), 1);
    assert!(!default_nix.contains("import (builtins"));
}

#[test]
fn unsupported_policies_are_reported_and_never_implemented() {
    let generated = run(vec![policy_set_input()]);

    let mut skipped: Vec<_> = generated
        .skipped
        .iter()
        .map(|s| (s.policy.name.as_str(), s.reason.code()))
        .collect();
    skipped.sort();

    assert_eq!(
        skipped,
        vec![
            ("audit-rules-present", "unrepresentable_expression"),
            ("block-critical-cves", "unsupported_policy_type"),
            ("require-physical-console-control", "not_native"),
        ]
    );

    // No skipped policy contributes an assignment.
    let manifest = manifest(&generated);
    let implemented_names: Vec<_> = manifest["policies"]
        .as_array()
        .expect("policies")
        .iter()
        .map(|p| p["name"].as_str().expect("name").to_string())
        .collect();
    for (name, _) in &skipped {
        assert!(!implemented_names.contains(&name.to_string()));
    }
}

#[test]
fn bundle_export_generates_only_the_selected_policy_versions() {
    let generated = run(vec![bundle_input()]);
    let names: Vec<_> = generated
        .implemented
        .iter()
        .map(|g| g.policy.name.as_str())
        .collect();
    assert_eq!(names, vec!["disable-empty-passwords", "require-time-sync"]);
    assert!(
        !generated
            .implemented
            .iter()
            .any(|g| g.policy.name == "unselected-baseline-policy"),
        "a deselected policy version must not be generated"
    );
}

#[test]
fn bundle_manifest_records_immutable_identities() {
    let generated = run(vec![bundle_input()]);
    let manifest = manifest(&generated);

    let bundle = &manifest["bundles"][0];
    assert_eq!(bundle["bundle_id"], "aaaaaaaa-0000-0000-0000-00000000000a");
    assert_eq!(
        bundle["bundle_version_id"],
        "bbbbbbbb-0000-0000-0000-00000000000b"
    );
    assert_eq!(bundle["framework"], "NIST SP 800-53");
    assert_eq!(
        bundle["semantic_digest"].as_str().expect("digest").len(),
        64
    );
    assert_eq!(
        bundle["selected_policy_version_ids"]
            .as_array()
            .expect("selection")
            .len(),
        2
    );

    for policy in manifest["policies"].as_array().expect("policies") {
        assert_eq!(
            policy["semantic_digest"].as_str().expect("digest").len(),
            64
        );
        assert_eq!(
            policy["bundle_version_ids"],
            serde_json::json!(["bbbbbbbb-0000-0000-0000-00000000000b"])
        );
    }
}

#[test]
fn compliance_mappings_are_metadata_and_do_not_duplicate_implementations() {
    let generated = run(vec![bundle_input()]);
    let policy = manifest_policy(&manifest(&generated), "require-time-sync");

    let mappings = policy["compliance_mappings"].as_array().expect("mappings");
    assert!(
        mappings
            .iter()
            .any(|m| m.as_str().unwrap_or_default().contains("CCI-001891")),
        "mapping metadata missing"
    );

    // One implementation regardless of how many requirements map to it.
    assert_eq!(
        policy["assignments"].as_array().expect("assignments").len(),
        1
    );
}

#[test]
fn combining_a_policy_export_and_a_bundle_export_merges_both() {
    let generated = run(vec![policy_set_input(), bundle_input()]);
    assert_eq!(generated.implemented.len(), 4);
}

/// Review finding 10.5: identical immutable content in two inputs must produce
/// byte-identical output regardless of the order the inputs were supplied.
#[test]
fn reordering_equivalent_inputs_produces_byte_identical_output() {
    let forward = run(vec![
        ("a.json", policy_set_json().into_bytes()),
        ("b.json", policy_set_json().into_bytes()),
    ]);
    let reverse = run(vec![
        ("b.json", policy_set_json().into_bytes()),
        ("a.json", policy_set_json().into_bytes()),
    ]);

    assert_eq!(forward.files, reverse.files, "generated bytes differ");

    // Provenance retains both origins deterministically.
    let policy = manifest_policy(&manifest(&forward), "require-firewall");
    assert_eq!(
        policy["source_inputs"],
        serde_json::json!(["a.json", "b.json"])
    );
}

#[test]
fn reordering_different_inputs_produces_byte_identical_output() {
    let forward = run(vec![policy_set_input(), bundle_input()]);
    let reverse = run(vec![bundle_input(), policy_set_input()]);
    assert_eq!(forward.files, reverse.files);
}

#[test]
fn repeated_generation_is_byte_identical() {
    let first = run(vec![policy_set_input(), bundle_input()]);
    let second = run(vec![policy_set_input(), bundle_input()]);
    assert_eq!(first.files, second.files);
}

#[test]
fn the_same_export_supplied_twice_is_deduplicated() {
    let generated = run(vec![
        ("a.json", policy_set_json().into_bytes()),
        ("b.json", policy_set_json().into_bytes()),
    ]);
    assert_eq!(generated.implemented.len(), 2);
}

#[test]
fn a_tampered_bundle_digest_is_rejected() {
    let xml = bundle_xccdf_xml();
    let marker = "canonical-model=\"cf-model-json-1\">";
    let index = xml.find(marker).expect("digest element") + marker.len();
    let mut tampered = xml.clone();
    let original = &xml[index..index + 1];
    let replacement = if original == "a" { "b" } else { "a" };
    tampered.replace_range(index..index + 1, replacement);

    let error = load_input(tampered.as_bytes(), "tampered.xml").expect_err("must be rejected");
    assert!(
        error.message.to_lowercase().contains("digest"),
        "expected a digest failure, got: {}",
        error.message
    );
}

#[test]
fn generated_artifact_references_no_crystal_forge_infrastructure() {
    let generated = run(vec![policy_set_input(), bundle_input()]);
    for generated_file in &generated.files {
        if generated_file.path == "manifest.json" {
            continue;
        }
        for forbidden in [
            "services.crystal-forge",
            "crystal-forge-agent",
            "inputs.crystal-forge",
            "fetchurl",
            "builtins.getFlake",
            "import <",
        ] {
            assert!(
                !generated_file.contents.contains(forbidden),
                "{} must not reference {forbidden}",
                generated_file.path
            );
        }
    }
}

#[test]
fn baseline_defaults_to_the_bundle_name() {
    let selection = resolve(vec![bundle_input()]);
    assert_eq!(
        derive_baseline(&selection, None),
        "crystal-forge-nixos-baseline"
    );
}
