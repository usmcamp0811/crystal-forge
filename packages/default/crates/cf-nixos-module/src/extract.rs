//! Recover declarative NixOS option assignments from policy implementations.
//!
//! Crystal Forge does not persist NixOS option assignments. A policy stores
//! *assertion* expressions that must evaluate to `true` against an evaluated
//! configuration, for example:
//!
//! ```text
//! config.services.openssh.settings.PermitRootLogin == "no"
//! ```
//!
//! This module inverts exactly that shape back into the assignment it asserts.
//! It never infers configuration from prose, shell commands, or remediation
//! text, and it never emits a partial implementation: if any expression in a
//! policy is not a plain option assertion, the whole policy is skipped with a
//! diagnostic.
//!
//! The accepted option-path and literal grammar is
//! [`cf_compliance::xccdf::inference`], shared with STIG fix-text inference, so
//! the two cannot drift apart.

use cf_compliance::xccdf::inference::{is_valid_nix_option_path, parse_nix_literal};

use crate::model::{OptionAssignment, ResolvedPolicy, SkipReason};

/// Accepted prefixes for an option path inside an assertion expression.
///
/// `cfg.config.` is the form produced by the server's Nix evaluation harness;
/// `config.` is the form written by the policy editor. Both denote the same
/// evaluated NixOS option.
const EXPRESSION_PREFIXES: [&str; 2] = ["cfg.config.", "config."];

/// Attempt to convert a policy into declarative NixOS option assignments.
///
/// Returns `Err(SkipReason)` whenever the policy cannot be represented, so the
/// caller can report it instead of silently omitting it.
pub fn extract_assignments(policy: &ResolvedPolicy) -> Result<Vec<OptionAssignment>, SkipReason> {
    // Only Crystal Forge-executable policies can carry a technical
    // implementation. manual / external / unbound / opaque never can.
    if policy.implementation_state != "native" {
        return Err(SkipReason::NotNative {
            implementation_state: policy.implementation_state.clone(),
        });
    }

    // `custom_check` is the only policy type whose configuration expresses
    // NixOS option state. `require_packages` stores bare package names, which
    // are not sound `pkgs` attribute paths, and every other native type is an
    // operational gate evaluated outside the module system.
    if policy.policy_type != "custom_check" {
        return Err(SkipReason::UnsupportedPolicyType {
            policy_type: policy.policy_type.clone(),
        });
    }

    let expressions = collect_expressions(&policy.config)?;
    if expressions.is_empty() {
        return Err(SkipReason::NoImplementation);
    }

    let mut assignments: Vec<OptionAssignment> = Vec::with_capacity(expressions.len());
    for expression in expressions {
        let assignment =
            invert_assertion(&expression).ok_or(SkipReason::UnrepresentableExpression {
                expression: expression.clone(),
            })?;

        // A policy that asserts two different values for one option cannot be
        // turned into a coherent module.
        if let Some(existing) = assignments
            .iter()
            .find(|candidate| candidate.option_path == assignment.option_path)
        {
            if existing.value != assignment.value {
                return Err(SkipReason::SelfContradictory {
                    option_path: assignment.option_path,
                    first: existing.value.nix_repr(),
                    second: assignment.value.nix_repr(),
                });
            }
            continue;
        }

        assignments.push(assignment);
    }

    // Deterministic emission order regardless of rule order in the export.
    assignments.sort_by(|a, b| a.option_path.cmp(&b.option_path));
    Ok(assignments)
}

/// Collect every assertion expression a `custom_check` config declares.
///
/// Supports the legacy single-expression form and the ordered multi-rule form.
/// An `any`-mode policy is rejected: it is satisfied by any one of its rules and
/// therefore does not determine a single configuration.
fn collect_expressions(config: &serde_json::Value) -> Result<Vec<String>, SkipReason> {
    let mut expressions = Vec::new();

    if let Some(rules) = config.get("rules").and_then(serde_json::Value::as_array) {
        if !rules.is_empty() {
            let mode = config
                .get("mode")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("all");
            if !mode.eq_ignore_ascii_case("all") {
                return Err(SkipReason::AmbiguousRuleMode {
                    mode: mode.to_string(),
                });
            }
            for rule in rules {
                if let Some(expression) = rule.get("expression").and_then(serde_json::Value::as_str)
                {
                    expressions.push(expression.to_string());
                }
            }
            return Ok(expressions);
        }
    }

    if let Some(expression) = config.get("expression").and_then(serde_json::Value::as_str) {
        if !expression.trim().is_empty() {
            expressions.push(expression.to_string());
        }
    }

    Ok(expressions)
}

/// Invert a single `<prefix><option.path> == <literal>` assertion.
///
/// Returns `None` for anything else, including comparisons other than `==`,
/// compound boolean expressions, function application, and interpolation.
fn invert_assertion(expression: &str) -> Option<OptionAssignment> {
    let trimmed = expression.trim();

    // Reject anything with additional structure before splitting, so that a
    // compound expression can never be reduced to one of its operands.
    if trimmed.contains("&&")
        || trimmed.contains("||")
        || trimmed.contains('!')
        || trimmed.contains(';')
        || trimmed.contains('(')
        || trimmed.contains('{')
        || trimmed.contains('[')
        || trimmed.contains("${")
        || trimmed.lines().count() != 1
    {
        return None;
    }

    let (raw_path, raw_value) = trimmed.split_once("==")?;

    // A second `==` means this is not a simple binary comparison.
    if raw_value.contains("==") {
        return None;
    }

    let path_with_prefix = raw_path.trim();
    let option_path = EXPRESSION_PREFIXES
        .iter()
        .find_map(|prefix| path_with_prefix.strip_prefix(prefix))?;

    if !is_valid_nix_option_path(option_path) {
        return None;
    }

    let value = parse_nix_literal(raw_value.trim())?;

    Some(OptionAssignment {
        option_path: option_path.to_string(),
        value,
    })
}

#[cfg(test)]
mod tests {
    use cf_compliance::xccdf::inference::NixosLiteralValue;
    use uuid::Uuid;

    use super::*;
    use crate::model::PolicyOrigin;

    fn policy(policy_type: &str, state: &str, config: serde_json::Value) -> ResolvedPolicy {
        ResolvedPolicy {
            policy_id: Uuid::nil(),
            policy_version_id: Uuid::nil(),
            version: "1".into(),
            name: "test-policy".into(),
            description: None,
            policy_type: policy_type.into(),
            implementation_state: state.into(),
            execution_phase: "nix-evaluation".into(),
            config,
            compliance_metadata: serde_json::json!({}),
            semantic_digest: String::new(),
            origin: PolicyOrigin {
                input_label: "input.json".into(),
                source_sha256: String::new(),
                bundle_version_id: None,
            },
        }
    }

    fn native_check(config: serde_json::Value) -> ResolvedPolicy {
        policy("custom_check", "native", config)
    }

    #[test]
    fn inverts_legacy_single_expression() {
        let p = native_check(serde_json::json!({
            "expression": "config.networking.firewall.enable == true",
        }));
        let assignments = extract_assignments(&p).expect("representable");
        assert_eq!(
            assignments,
            vec![OptionAssignment {
                option_path: "networking.firewall.enable".into(),
                value: NixosLiteralValue::Boolean(true),
            }]
        );
    }

    #[test]
    fn accepts_the_cfg_config_harness_prefix() {
        let p = native_check(serde_json::json!({
            "expression": "cfg.config.services.openssh.enable == true",
        }));
        let assignments = extract_assignments(&p).expect("representable");
        assert_eq!(assignments[0].option_path, "services.openssh.enable");
    }

    #[test]
    fn inverts_string_and_integer_literals() {
        let p = native_check(serde_json::json!({
            "mode": "all",
            "rules": [
                {"expression": "config.services.openssh.settings.PermitRootLogin == \"no\""},
                {"expression": "config.services.openssh.ports == 22"},
            ],
        }));
        let assignments = extract_assignments(&p).expect("representable");
        assert_eq!(assignments.len(), 2);
        assert_eq!(
            assignments[0],
            OptionAssignment {
                option_path: "services.openssh.ports".into(),
                value: NixosLiteralValue::Integer(22),
            }
        );
        assert_eq!(
            assignments[1],
            OptionAssignment {
                option_path: "services.openssh.settings.PermitRootLogin".into(),
                value: NixosLiteralValue::StringLiteral("no".into()),
            }
        );
    }

    #[test]
    fn assignments_are_sorted_regardless_of_rule_order() {
        let forward = native_check(serde_json::json!({
            "rules": [
                {"expression": "config.a.z == true"},
                {"expression": "config.a.a == true"},
            ],
        }));
        let reverse = native_check(serde_json::json!({
            "rules": [
                {"expression": "config.a.a == true"},
                {"expression": "config.a.z == true"},
            ],
        }));
        assert_eq!(
            extract_assignments(&forward).expect("ok"),
            extract_assignments(&reverse).expect("ok")
        );
    }

    #[test]
    fn duplicate_identical_assertions_collapse() {
        let p = native_check(serde_json::json!({
            "rules": [
                {"expression": "config.a.b == true"},
                {"expression": "config.a.b == true"},
            ],
        }));
        assert_eq!(extract_assignments(&p).expect("ok").len(), 1);
    }

    #[test]
    fn self_contradictory_policy_is_skipped() {
        let p = native_check(serde_json::json!({
            "rules": [
                {"expression": "config.a.b == true"},
                {"expression": "config.a.b == false"},
            ],
        }));
        assert!(matches!(
            extract_assignments(&p),
            Err(SkipReason::SelfContradictory { .. })
        ));
    }

    #[test]
    fn any_mode_is_skipped_as_ambiguous() {
        let p = native_check(serde_json::json!({
            "mode": "any",
            "rules": [{"expression": "config.a.b == true"}],
        }));
        assert!(matches!(
            extract_assignments(&p),
            Err(SkipReason::AmbiguousRuleMode { .. })
        ));
    }

    #[test]
    fn manual_policy_is_skipped() {
        let p = policy("custom_check", "manual", serde_json::json!({}));
        assert!(matches!(
            extract_assignments(&p),
            Err(SkipReason::NotNative { .. })
        ));
    }

    #[test]
    fn unbound_and_opaque_policies_are_skipped() {
        for state in ["unbound", "opaque", "external"] {
            let p = policy("custom_check", state, serde_json::json!({}));
            assert!(
                matches!(extract_assignments(&p), Err(SkipReason::NotNative { .. })),
                "state {state} should be skipped"
            );
        }
    }

    #[test]
    fn operational_policy_types_are_skipped() {
        for policy_type in [
            "require_cve_check",
            "time_window",
            "require_approvals",
            "canary_rollout",
            "cve_threshold",
            "require_cf_agent",
            "imported_xccdf",
        ] {
            let p = policy(policy_type, "native", serde_json::json!({}));
            assert!(
                matches!(
                    extract_assignments(&p),
                    Err(SkipReason::UnsupportedPolicyType { .. })
                ),
                "type {policy_type} should be skipped"
            );
        }
    }

    #[test]
    fn require_packages_is_skipped_rather_than_guessed() {
        let p = policy(
            "require_packages",
            "native",
            serde_json::json!({"packages": ["auditd"], "strict": true}),
        );
        match extract_assignments(&p) {
            Err(SkipReason::UnsupportedPolicyType { policy_type }) => {
                assert_eq!(policy_type, "require_packages");
            }
            other => panic!("expected an unsupported-type skip, got {other:?}"),
        }
    }

    #[test]
    fn policy_without_expression_is_skipped() {
        let p = native_check(serde_json::json!({"strict": true}));
        assert!(matches!(
            extract_assignments(&p),
            Err(SkipReason::NoImplementation)
        ));
    }

    #[test]
    fn arbitrary_nix_is_never_converted() {
        let unrepresentable = [
            "builtins.all (x: x) config.foo",
            "config.a.b == true && config.c.d == false",
            "config.a.b != true",
            "config.a.b == \"${var}\"",
            "config.a.b >= 3",
            "let x = 1; in config.a.b == x",
            "config.a.b == (1 + 2)",
            "config.a.b == [ 1 2 ]",
            "config.a.b == { c = 1; }",
            "!config.a.b == true",
            "somethingElse.a.b == true",
            "config.a..b == true",
        ];
        for expression in unrepresentable {
            let p = native_check(serde_json::json!({"expression": expression}));
            assert!(
                matches!(
                    extract_assignments(&p),
                    Err(SkipReason::UnrepresentableExpression { .. })
                ),
                "expression should be rejected: {expression}"
            );
        }
    }

    #[test]
    fn a_partially_representable_policy_is_skipped_entirely() {
        let p = native_check(serde_json::json!({
            "rules": [
                {"expression": "config.a.b == true"},
                {"expression": "builtins.length config.users.users > 0"},
            ],
        }));
        assert!(
            matches!(
                extract_assignments(&p),
                Err(SkipReason::UnrepresentableExpression { .. })
            ),
            "a policy must never be emitted with only part of its rules"
        );
    }
}
