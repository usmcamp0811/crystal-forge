//! Integration tests: exported artifact -> generated NixOS module.
//!
//! These drive the same layers the CLI uses, over real export documents built
//! by the fixture module (including real `cf-model-json-1` semantic digests).
//! Evaluation of the generated Nix through the NixOS module system is covered
//! by the `nixos-module-generation` flake check.

use cf_nixos_module::fixture::{bundle_xccdf_xml, policy_set_json};
use cf_nixos_module::generate::{Generated, Layout, generate};
use cf_nixos_module::input::{LoadedInput, load_input};
use cf_nixos_module::select::select_policies;

fn run(inputs: Vec<(&str, Vec<u8>)>) -> Generated {
    let loaded: Vec<LoadedInput> = inputs
        .into_iter()
        .map(|(label, bytes)| load_input(&bytes, label).expect("input loads"))
        .collect();
    let selection = select_policies(&loaded).expect("no identity conflict");
    generate(&selection, Layout::Directory).expect("no option conflict")
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

#[test]
fn policy_set_export_generates_expected_option_assignments() {
    let generated = run(vec![policy_set_input()]);

    let names: Vec<_> = generated
        .implemented
        .iter()
        .map(|g| g.policy.name.as_str())
        .collect();
    assert_eq!(names, vec!["disable-root-ssh-login", "require-firewall"]);

    let ssh = file(&generated, "policies/disable-root-ssh-login-22222222.nix");
    assert!(
        ssh.contains(r#"services.openssh.settings.PermitRootLogin = "no";"#),
        "{ssh}"
    );
    assert!(
        ssh.contains("services.openssh.settings.PasswordAuthentication = false;"),
        "{ssh}"
    );

    let firewall = file(&generated, "policies/require-firewall-22222222.nix");
    assert!(
        firewall.contains("networking.firewall.enable = true;"),
        "{firewall}"
    );
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

    // None of the skipped policies may appear anywhere in the generated Nix.
    for generated_file in &generated.files {
        if generated_file.path == "manifest.json" {
            continue;
        }
        for (name, _) in &skipped {
            assert!(
                !generated_file.contents.contains(name),
                "skipped policy {name} leaked into {}",
                generated_file.path
            );
        }
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

    // The bundle version deselects this policy, so it must not be generated.
    assert!(
        !generated
            .implemented
            .iter()
            .any(|g| g.policy.name == "unselected-baseline-policy"),
        "a deselected policy version must not be generated"
    );
}

#[test]
fn bundle_manifest_records_immutable_bundle_and_policy_identities() {
    let generated = run(vec![bundle_input()]);
    let manifest: serde_json::Value =
        serde_json::from_str(file(&generated, "manifest.json")).expect("valid JSON");

    let bundle = &manifest["bundles"][0];
    assert_eq!(bundle["bundle_id"], "aaaaaaaa-0000-0000-0000-00000000000a");
    assert_eq!(
        bundle["bundle_version_id"],
        "bbbbbbbb-0000-0000-0000-00000000000b"
    );
    assert_eq!(bundle["framework"], "NIST SP 800-53");
    assert_eq!(bundle["publication_state"], "accepted");
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
    assert_eq!(
        bundle["source_export_digest"]
            .as_str()
            .expect("source digest")
            .len(),
        64
    );

    for policy in manifest["policies"].as_array().expect("policies") {
        assert_eq!(
            policy["bundle_version_id"],
            "bbbbbbbb-0000-0000-0000-00000000000b"
        );
        assert_eq!(
            policy["semantic_digest"].as_str().expect("digest").len(),
            64
        );
    }
}

#[test]
fn compliance_mappings_are_preserved_without_duplicating_the_implementation() {
    let generated = run(vec![bundle_input()]);
    let module = file(&generated, "policies/require-time-sync-22222222.nix");

    assert!(module.contains("CCI-001891"), "{module}");
    assert_eq!(
        module.matches("services.timesyncd.enable =").count(),
        1,
        "the implementation must be emitted exactly once"
    );
}

#[test]
fn combining_a_policy_export_and_a_bundle_export_merges_both() {
    let generated = run(vec![policy_set_input(), bundle_input()]);
    assert_eq!(generated.implemented.len(), 4);

    let default_nix = file(&generated, "default.nix");
    for expected in [
        "disable-empty-passwords",
        "disable-root-ssh-login",
        "require-firewall",
        "require-time-sync",
    ] {
        assert!(default_nix.contains(expected), "{default_nix}");
    }
}

#[test]
fn generation_is_deterministic_and_independent_of_input_order() {
    let forward = run(vec![policy_set_input(), bundle_input()]);
    let reverse = run(vec![bundle_input(), policy_set_input()]);
    assert_eq!(forward.files, reverse.files);

    // And byte-stable across repeated runs.
    let again = run(vec![policy_set_input(), bundle_input()]);
    assert_eq!(forward.files, again.files);
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
    // Flip one character of the bundle content digest.
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
fn generated_modules_reference_no_crystal_forge_infrastructure() {
    let generated = run(vec![policy_set_input(), bundle_input()]);
    for generated_file in &generated.files {
        if generated_file.path == "manifest.json" {
            continue;
        }
        let contents = &generated_file.contents;
        for forbidden in [
            "services.crystal-forge",
            "crystal-forge-agent",
            "inputs.crystal-forge",
            "fetchurl",
            "builtins.getFlake",
            "import <",
        ] {
            assert!(
                !contents.contains(forbidden),
                "{} must not reference {forbidden}",
                generated_file.path
            );
        }
    }
}
