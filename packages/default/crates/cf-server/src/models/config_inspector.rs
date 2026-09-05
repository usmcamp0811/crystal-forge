//! Builds and decodes the isolated, one-configuration Config inspector.
//!
//! This module is intentionally separate from the primary evaluator. The
//! generated expression selects one `nixosConfigurations` attribute before it
//! walks that configuration's option tree. It does not enumerate sibling
//! configurations or inspect exported modules.

use anyhow::{Context, Result};

use super::deployment_policies::SNAPSHOT_EXTRACTION_PRELUDE;
use super::evaluation_snapshots::EvaluatedOption;

/// Builds the real-Nix expression for one exact configuration revision.
///
/// The returned expression produces a JSON-compatible list of
/// [`EvaluatedOption`] values when evaluated with `nix eval --json`.
///
/// The configuration name is selected with `builtins.getAttr`; the expression
/// never traverses the sibling configuration names or `flake.nixosModules`.
pub(crate) fn build_inspector_expression(flake_ref: &str, configuration_name: &str) -> String {
    let flake_ref = nix_string(flake_ref);
    let configuration_name = nix_string(configuration_name);

    format!(
        r#"
let
  flake = builtins.getFlake {flake_ref};
  configuration = builtins.getAttr {configuration_name} flake.nixosConfigurations;
  lib = flake.inputs.nixpkgs.lib;
{prelude}
in
  builtins.map
    (safeOptionSnapshot lib {{}} [] )
    (walkOptions 0 [] configuration.options)
"#,
        flake_ref = flake_ref,
        configuration_name = configuration_name,
        prelude = SNAPSHOT_EXTRACTION_PRELUDE,
    )
}

/// Parses the inspector's JSON output into the existing snapshot domain.
///
/// # Errors
///
/// Returns an error when Nix emits malformed JSON or a value outside the
/// [`EvaluatedOption`] wire contract. No persistence or redaction occurs here;
/// callers apply [`EvaluatedOption::redacted`] before either operation.
pub(crate) fn parse_inspector_output(output: &[u8]) -> Result<Vec<EvaluatedOption>> {
    serde_json::from_slice(output)
        .context("targeted Config inspector output was not valid option JSON")
}

fn nix_string(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "\"\"".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::evaluation_snapshots::{
        OptionDefinitionProvenance, SafeEvaluationError, SafeOptionValue,
    };
    use serde_json::json;

    #[test]
    fn expression_selects_one_configuration_without_primary_evaluator_tokens() {
        let expression = build_inspector_expression("path:/tmp/example", "good");

        assert!(expression.contains("builtins.getAttr \"good\" flake.nixosConfigurations"));
        assert!(!expression.contains("flake.nixosModules"));
        assert!(!expression.contains("evaluationSnapshot"));
    }

    #[test]
    fn wire_output_preserves_typed_values_and_provenance() {
        let wire = json!([
            {
                "path": "services.example.enable",
                "declared_type": "boolean",
                "value": { "kind": "scalar", "value": true },
                "definitions": [{
                    "source_path": "/flake/module.nix",
                    "source_input": "self",
                    "source_revision": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                    "value": { "kind": "scalar", "value": true },
                    "winning": true,
                    "priority": 100,
                    "status": "winning"
                }],
                "overridden": false
            },
            {
                "path": "services.example.package",
                "declared_type": "package",
                "value": { "kind": "opaque", "value": { "type_name": "derivation" } },
                "definitions": [],
                "overridden": false
            },
            {
                "path": "services.example.failed",
                "declared_type": "unknown",
                "value": {
                    "kind": "failed",
                    "value": { "code": "not_evaluated", "message": "Option could not be inspected" }
                },
                "definitions": [],
                "overridden": false
            }
        ]);

        let options = parse_inspector_output(&serde_json::to_vec(&wire).unwrap()).unwrap();
        assert_eq!(options.len(), 3);
        assert!(matches!(options[0].value, SafeOptionValue::Scalar(_)));
        assert!(matches!(options[1].value, SafeOptionValue::Opaque { .. }));
        assert!(matches!(
            options[2].value,
            SafeOptionValue::Failed(SafeEvaluationError { .. })
        ));
        assert_eq!(options[0].definitions.len(), 1);
        assert_eq!(
            options[0].definitions[0].source_input.as_deref(),
            Some("self")
        );
        assert_eq!(options[0].definitions[0].priority, Some(100));
    }

    #[test]
    fn wire_output_preserves_multiple_definitions() {
        let wire = json!([{
            "path": "example.value",
            "declared_type": "string",
            "value": { "kind": "scalar", "value": "winner" },
            "definitions": [
                {
                    "source_path": "/first.nix",
                    "value": { "kind": "scalar", "value": "overridden" },
                    "winning": false,
                    "priority": 100,
                    "status": "overridden"
                },
                {
                    "source_path": "/second.nix",
                    "value": { "kind": "scalar", "value": "winner" },
                    "winning": true,
                    "priority": 50,
                    "status": "winning"
                }
            ],
            "overridden": true
        }]);

        let options = parse_inspector_output(&serde_json::to_vec(&wire).unwrap()).unwrap();
        let definitions: &[OptionDefinitionProvenance] = &options[0].definitions;
        assert_eq!(definitions.len(), 2);
        assert!(definitions.iter().any(|definition| definition.winning));
        assert!(definitions.iter().any(|definition| !definition.winning));
        assert!(options[0].overridden);
    }

    #[test]
    #[ignore = "requires the repository's real Nix toolchain"]
    fn real_nix_fixture_inspects_only_good_configuration() {
        let fixture = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../../checks/config-inspector/fixture");
        let flake_ref = format!("path:{}", fixture.display());
        let expression = build_inspector_expression(&flake_ref, "good");
        let output = std::process::Command::new("nix")
            .args(["eval", "--impure", "--json", "--expr", &expression])
            .output()
            .expect("real Nix must be available for the inspector fixture");

        assert!(
            output.status.success(),
            "targeted inspector failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let options = parse_inspector_output(&output.stdout).expect("fixture output must parse");
        assert!(!options.is_empty());
        assert!(
            options
                .iter()
                .any(|option| option.path.contains("crystalForge")),
            "fixture custom options were not present: {:?}",
            options
                .iter()
                .map(|option| &option.path)
                .collect::<Vec<_>>()
        );
        assert!(!options
            .iter()
            .any(|option| option.path.contains("unrelated")));
        assert!(options
            .iter()
            .any(|option| option.path == "crystalForgeInspector.overridden"));

        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(!stderr.contains("unrelated configuration forced"));
        assert!(!stderr.contains("namespace-change-me"));
    }
}
