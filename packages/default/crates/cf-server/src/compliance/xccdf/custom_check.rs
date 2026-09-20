//! Projects and reconciles CF-XCCDF custom-check configuration.
//!
//! `config-json` is the lossless persisted configuration. The typed XML form
//! is a redundant executable projection and must agree with the JSON before a
//! native import can persist or execute it.

use serde_json::Value;
use uuid::Uuid;

use super::models::{CfCustomCheck, CfCustomCheckRule};
use crate::models::custom_check::ExpressionBinding;

/// Identifies the current custom-check context emitted by Crystal Forge.
pub const CURRENT_CONTEXT: &str = "nixos-configuration-v2";
/// Identifies the current custom-check expression binding.
pub const CURRENT_BINDING: &str = "config";

const LEGACY_CONTEXT: &str = "nixos-configuration-v1";
const LEGACY_BINDING: &str = "cfg";

/// Returns the expression binding declared by a typed custom-check.
///
/// # Errors
///
/// Returns an error when context or binding is missing or the pair is not a
/// supported V1 or V2 contract.
pub fn expression_binding(typed: &CfCustomCheck) -> Result<ExpressionBinding, String> {
    match (typed.context.as_deref(), typed.binding.as_deref()) {
        (Some(CURRENT_CONTEXT), Some(CURRENT_BINDING)) => Ok(ExpressionBinding::Current),
        (Some(LEGACY_CONTEXT), Some(LEGACY_BINDING)) => Ok(ExpressionBinding::Legacy),
        (None, _) => Err("custom-check context is required".into()),
        (_, None) => Err("custom-check binding is required".into()),
        _ => Err("custom-check context and binding pair is unsupported".into()),
    }
}

/// Gives one effective typed custom-check rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomCheckRuleProjection {
    /// Identifies the evaluator result field.
    pub field_name: String,
    /// Determines whether a failed rule blocks deployment.
    pub strict: bool,
    /// Gives operator-facing detail for the rule.
    pub description: String,
    /// Gives a canonical Nix expression with `config` in scope.
    pub expression: String,
}

/// Gives the effective typed projection of persisted custom-check JSON.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomCheckProjection {
    /// Selects `all` or `any` aggregation.
    pub mode: String,
    /// Preserves effective rules in evaluation order.
    pub rules: Vec<CustomCheckRuleProjection>,
}

/// Builds the runtime-compatible typed projection of canonical config JSON.
///
/// Missing mode defaults to `all`. Non-empty rules take precedence. A
/// top-level expression takes precedence over missing or empty rules and uses
/// runtime mode `all`. An absent expression with empty `all` rules projects no
/// enforcement.
///
/// # Errors
///
/// Returns an error for malformed mode or rule fields, empty `any`, a missing
/// legacy expression, or executable use of the historical `cfg.config` binding.
pub fn project_custom_check(
    policy_name: &str,
    policy_id: Uuid,
    policy_description: Option<&str>,
    config: &Value,
) -> Result<CustomCheckProjection, String> {
    let config = crate::models::custom_check::validate_and_normalize_config(
        config,
        ExpressionBinding::Current,
        false,
    )?;
    if let Some(rules) = config
        .get("rules")
        .and_then(Value::as_array)
        .filter(|rules| !rules.is_empty())
    {
        let configured_mode = match config.get("mode") {
            None => "all",
            Some(Value::String(mode)) if matches!(mode.as_str(), "all" | "any") => mode.as_str(),
            Some(Value::String(_)) => return Err("mode must be all or any".into()),
            Some(_) => return Err("mode must be a string".into()),
        };
        let rules = rules
            .iter()
            .enumerate()
            .map(|(index, rule)| project_rule(rule, index))
            .collect::<Result<Vec<_>, _>>()?;
        return Ok(CustomCheckProjection {
            mode: configured_mode.into(),
            rules,
        });
    }

    let expression = config
        .get("expression")
        .and_then(Value::as_str)
        .filter(|expression| !expression.is_empty());
    let Some(expression) = expression else {
        return Ok(CustomCheckProjection {
            mode: "all".into(),
            rules: Vec::new(),
        });
    };
    require_canonical_expression(expression)?;
    let field_name = config
        .get("field_name")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .unwrap_or_else(|| runtime_field_name(policy_name, policy_id));
    let strict = config
        .get("strict")
        .map(|value| {
            value
                .as_bool()
                .ok_or_else(|| "strict must be a Boolean".to_string())
        })
        .transpose()?
        .unwrap_or(false);
    let description = config
        .get("description")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .or_else(|| policy_description.map(str::to_owned))
        .unwrap_or_else(|| format!("Custom policy: {policy_name}"));
    Ok(CustomCheckProjection {
        // Runtime ignores config.mode for the single-expression shape.
        mode: "all".into(),
        rules: vec![CustomCheckRuleProjection {
            field_name,
            strict,
            description,
            expression: expression.into(),
        }],
    })
}

fn project_rule(rule: &Value, index: usize) -> Result<CustomCheckRuleProjection, String> {
    let object = rule
        .as_object()
        .ok_or_else(|| format!("rules[{index}] must be an object"))?;
    let expression = object
        .get("expression")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("rules[{index}].expression is required"))?;
    require_canonical_expression(expression)?;
    let field_name = object
        .get("field_name")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("rules[{index}].field_name is required"))?;
    let strict = object
        .get("strict")
        .map(|value| {
            value
                .as_bool()
                .ok_or_else(|| format!("rules[{index}].strict must be a Boolean"))
        })
        .transpose()?
        .unwrap_or(true);
    let description = object
        .get("description")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .unwrap_or_else(|| format!("rule_{index}"));
    Ok(CustomCheckRuleProjection {
        field_name: field_name.into(),
        strict,
        description,
        expression: expression.into(),
    })
}

fn require_canonical_expression(expression: &str) -> Result<(), String> {
    if crate::models::custom_check::has_legacy_reference(expression) {
        Err("expression uses the historical cfg.config binding".into())
    } else {
        Ok(())
    }
}

/// Reconciles a parsed typed custom-check with its canonical JSON projection.
///
/// V1 accepts only the `nixos-configuration-v1` and `cfg` pair and normalizes
/// executable `cfg.config.*` references. V2 accepts only the current pair and
/// requires canonical expression text. String and comment text is not treated
/// as an executable reference.
///
/// # Errors
///
/// Returns an error for an unknown or contradictory context/binding pair,
/// lexical inconsistency, malformed typed fields, empty `any`, or a semantic
/// contradiction between typed XML and `config-json`.
pub fn reconcile_custom_check(
    typed: &CfCustomCheck,
    expected: &CustomCheckProjection,
) -> Result<(), String> {
    let legacy = expression_binding(typed)? == ExpressionBinding::Legacy;
    let mode = typed
        .mode
        .as_deref()
        .ok_or_else(|| "custom-check mode is required".to_string())?;
    if !matches!(mode, "all" | "any") {
        return Err("custom-check mode must be all or any".into());
    }
    if mode == "any" && typed.rules.is_empty() {
        return Err("custom-check mode any requires at least one rule".into());
    }

    let actual = CustomCheckProjection {
        mode: mode.into(),
        rules: typed
            .rules
            .iter()
            .enumerate()
            .map(|(index, rule)| {
                let expected_rule = expected
                    .rules
                    .get(index)
                    .ok_or_else(|| "typed custom-check contradicts config-json".to_string())?;
                parsed_rule(rule, expected_rule, index, legacy)
            })
            .collect::<Result<Vec<_>, _>>()?,
    };
    if &actual != expected {
        return Err("typed custom-check contradicts config-json".into());
    }
    Ok(())
}

fn parsed_rule(
    rule: &CfCustomCheckRule,
    expected: &CustomCheckRuleProjection,
    index: usize,
    legacy: bool,
) -> Result<CustomCheckRuleProjection, String> {
    let expression = rule
        .expression
        .as_deref()
        .ok_or_else(|| format!("typed rule {index} expression is required"))?;
    if rule.language.as_deref() != Some("nix") {
        return Err(format!("typed rule {index} language must be nix"));
    }
    let expression = if legacy {
        if crate::models::custom_check::has_current_reference(expression) {
            return Err(format!("typed V1 rule {index} uses the V2 config binding"));
        }
        crate::models::custom_check::normalize_expression(expression).0
    } else {
        require_canonical_expression(expression)?;
        expression.into()
    };
    Ok(CustomCheckRuleProjection {
        field_name: rule
            .field_name
            .clone()
            .ok_or_else(|| format!("typed rule {index} field-name is required"))?,
        strict: rule
            .strict
            .ok_or_else(|| format!("typed rule {index} strict is required"))?,
        description: rule
            .description
            .clone()
            .unwrap_or_else(|| expected.description.clone()),
        expression,
    })
}

fn runtime_field_name(name: &str, id: Uuid) -> String {
    let mut slug = name
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>();
    while slug.contains("__") {
        slug = slug.replace("__", "_");
    }
    let slug = slug.trim_matches('_');
    let id = id.to_string();
    let short_id = &id[..8.min(id.len())];
    if slug.is_empty() {
        format!("custom_{short_id}")
    } else {
        format!("{slug}_{short_id}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn typed(context: &str, binding: &str, expression: &str) -> CfCustomCheck {
        CfCustomCheck {
            mode: Some("all".into()),
            context: Some(context.into()),
            binding: Some(binding.into()),
            rules: vec![CfCustomCheckRule {
                field_name: Some("enabled".into()),
                strict: Some(true),
                description: Some("Enabled".into()),
                expression: Some(expression.into()),
                language: Some("nix".into()),
            }],
        }
    }

    fn projection() -> CustomCheckProjection {
        project_custom_check(
            "Policy",
            Uuid::nil(),
            None,
            &json!({
                "mode": "all",
                "rules": [{
                    "field_name": "enabled",
                    "strict": true,
                    "description": "Enabled",
                    "expression": "config.services.example.enable"
                }]
            }),
        )
        .unwrap()
    }

    #[test]
    fn reconciles_v1_and_v2_lexical_contracts() {
        let expected = projection();
        reconcile_custom_check(
            &typed(
                LEGACY_CONTEXT,
                LEGACY_BINDING,
                "cfg.config.services.example.enable",
            ),
            &expected,
        )
        .unwrap();
        reconcile_custom_check(
            &typed(
                CURRENT_CONTEXT,
                CURRENT_BINDING,
                &expected.rules[0].expression,
            ),
            &expected,
        )
        .unwrap();
    }

    #[test]
    fn optional_typed_description_uses_the_json_runtime_default() {
        let expected = projection();
        let mut legacy = typed(
            LEGACY_CONTEXT,
            LEGACY_BINDING,
            "cfg.config.services.example.enable",
        );
        legacy.rules[0].description = None;

        reconcile_custom_check(&legacy, &expected).unwrap();
    }

    #[test]
    fn rejects_pair_lexical_and_json_contradictions() {
        let expected = projection();
        assert!(
            reconcile_custom_check(
                &typed(
                    CURRENT_CONTEXT,
                    LEGACY_BINDING,
                    &expected.rules[0].expression
                ),
                &expected,
            )
            .is_err()
        );
        assert!(
            reconcile_custom_check(
                &typed(
                    LEGACY_CONTEXT,
                    LEGACY_BINDING,
                    &expected.rules[0].expression
                ),
                &expected,
            )
            .is_err()
        );
        assert!(
            reconcile_custom_check(
                &typed(
                    CURRENT_CONTEXT,
                    CURRENT_BINDING,
                    "config.services.other.enable",
                ),
                &expected,
            )
            .is_err()
        );
    }

    #[test]
    fn empty_all_is_no_enforcement_and_empty_any_is_invalid() {
        let all = project_custom_check(
            "Policy",
            Uuid::nil(),
            None,
            &json!({"mode": "all", "rules": []}),
        )
        .unwrap();
        assert!(all.rules.is_empty());
        assert!(
            project_custom_check(
                "Policy",
                Uuid::nil(),
                None,
                &json!({"mode": "any", "rules": []}),
            )
            .is_err()
        );
    }

    #[test]
    fn legacy_single_expression_uses_runtime_projection_defaults() {
        let policy_id = Uuid::parse_str("11111111-2222-3333-4444-555555555555").unwrap();
        let projected = project_custom_check(
            "Example Policy",
            policy_id,
            Some("Policy description"),
            &json!({"expression": "config.services.example.enable"}),
        )
        .unwrap();

        assert_eq!(projected.mode, "all");
        assert_eq!(projected.rules.len(), 1);
        assert_eq!(projected.rules[0].field_name, "example_policy_11111111");
        assert!(!projected.rules[0].strict);
        assert_eq!(projected.rules[0].description, "Policy description");
    }

    #[test]
    fn expression_precedes_empty_rules_and_nonempty_rules_precede_expression() {
        let expression = project_custom_check(
            "Policy",
            Uuid::nil(),
            None,
            &json!({
                "mode": "any",
                "expression": "config.single",
                "field_name": "single",
                "rules": []
            }),
        )
        .unwrap();
        assert_eq!(expression.mode, "all");
        assert_eq!(expression.rules.len(), 1);
        assert_eq!(expression.rules[0].field_name, "single");

        let rules = project_custom_check(
            "Policy",
            Uuid::nil(),
            None,
            &json!({
                "expression": "config.single",
                "rules": [{"field_name": "nested", "expression": "config.nested"}]
            }),
        )
        .unwrap();
        assert_eq!(rules.rules.len(), 1);
        assert_eq!(rules.rules[0].field_name, "nested");

        let malformed_irrelevant = project_custom_check(
            "Policy",
            Uuid::nil(),
            None,
            &json!({
                "mode": "any",
                "strict": "ignored",
                "expression": {"malformed": true},
                "field_name": 42,
                "description": false,
                "rules": [{"field_name": "nested", "expression": "config.nested"}]
            }),
        )
        .expect("effective rules must supersede malformed top-level expression fields");
        assert_eq!(malformed_irrelevant.mode, "any");
        assert_eq!(malformed_irrelevant.rules.len(), 1);
        assert_eq!(malformed_irrelevant.rules[0].field_name, "nested");
    }
}
