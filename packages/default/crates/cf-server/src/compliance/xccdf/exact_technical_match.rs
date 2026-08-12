//! Exact technical enforcement matching for policy candidates.
//!
//! This module extracts canonical technical enforcement semantics from STIG
//! requirement fix text and matches them against existing policy configurations.
//!
//! # Design
//!
//! Technical enforcement identity is derived from normalized NixOS option
//! assignments extracted from STIG fix text via `infer_nixos_assertions()`.
//!
//! A policy is an exact technical match if its `config` JSON contains identical
//! option paths with identical expected values as the imported requirement.
//!
//! Matching is deterministic and database-independent for testing; the query
//! layer performs the actual database lookup.

use serde_json::{Map, Value, json};
use uuid::Uuid;

use super::inference::{NixosLiteralValue, NixosOptionAssertionDraft, infer_nixos_assertions};

/// Canonical technical enforcement identity extracted from a requirement.
///
/// This represents the normalized, comparable form of what a STIG requirement
/// asserts about system configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequirementTechnicalIdentity {
    /// Sorted collection of option paths → expected values.
    /// Example: `{ "services.openssh.settings.PermitRootLogin": "no" }`
    pub enforced_options: Map<String, Value>,
}

impl RequirementTechnicalIdentity {
    /// Construct a technical identity from a set of inferred NixOS assertions.
    pub fn from_assertions(assertions: Vec<NixosOptionAssertionDraft>) -> Self {
        let mut enforced_options = Map::new();

        for assertion in assertions {
            // Convert the inferred value to a canonical JSON representation.
            let json_value = match &assertion.expected_value {
                NixosLiteralValue::Boolean(b) => Value::Bool(*b),
                NixosLiteralValue::Integer(i) => Value::Number((*i).into()),
                NixosLiteralValue::StringLiteral(s) => Value::String(s.clone()),
            };

            enforced_options.insert(assertion.option_path.clone(), json_value);
        }

        RequirementTechnicalIdentity { enforced_options }
    }

    /// Construct a technical identity from fix text.
    pub fn from_fix_text(fix_text: &str) -> Self {
        let assertions = infer_nixos_assertions(fix_text);
        Self::from_assertions(assertions)
    }

    /// Check whether a policy configuration implements this requirement's technical enforcement.
    ///
    /// Returns `true` only if every enforced option in the requirement is present
    /// in the policy config with the exact same value. The policy may have
    /// additional options; those do not affect the match.
    pub fn is_implemented_by(&self, policy_config: &Value) -> bool {
        // If the requirement has no technical enforcement, it's trivially implemented.
        if self.enforced_options.is_empty() {
            return false; // Don't claim match if no enforcement inferred
        }

        // Policy config should be an object.
        let Some(config_obj) = policy_config.as_object() else {
            return false;
        };

        // Every required option must be present and identical.
        for (option_path, required_value) in &self.enforced_options {
            match config_obj.get(option_path) {
                Some(policy_value) if policy_value == required_value => {
                    // This option matches; check the next one.
                }
                _ => {
                    // Option missing or value mismatch.
                    return false;
                }
            }
        }

        true
    }

    /// Get a human-readable description of the enforced options.
    pub fn description(&self) -> String {
        if self.enforced_options.is_empty() {
            return "(no technical enforcement inferred)".to_string();
        }

        let items: Vec<String> = self
            .enforced_options
            .iter()
            .map(|(path, value)| {
                let val_str = match value {
                    Value::Bool(b) => b.to_string(),
                    Value::Number(n) => n.to_string(),
                    Value::String(s) => format!("\"{}\"", s),
                    _ => value.to_string(),
                };
                format!("{} = {}", path, val_str)
            })
            .collect();

        items.join("; ")
    }
}

/// DB query result for a policy version with its configuration.
#[derive(Debug, Clone)]
pub struct PolicyConfigRow {
    pub policy_id: Uuid,
    pub policy_version_id: Uuid,
    pub policy_name: String,
    pub config: Value,
}

/// Find all current-accepted policies whose config implements the requirement's technical enforcement.
///
/// This function is database-aware and performs the actual lookup.
/// It should only be called after the technical identity is known.
///
/// The returned candidates are *unordered* and must be deduplicated against
/// authoritative and inherited candidates by the caller.
pub async fn find_exact_technical_match_candidates(
    pool: &sqlx::PgPool,
    technical_identity: &RequirementTechnicalIdentity,
) -> anyhow::Result<Vec<PolicyConfigRow>> {
    // If there's no technical enforcement to match, return no candidates.
    if technical_identity.enforced_options.is_empty() {
        return Ok(vec![]);
    }

    // Fetch all current-accepted policy versions with their config.
    // We'll filter in-process since JSON matching in SQL would be complex
    // and we want to keep the query simple.
    let rows: Vec<(Uuid, Uuid, String, Value)> = sqlx::query_as(
        r#"
        SELECT DISTINCT dp.id, pv.id, pv.name, pv.config
        FROM deployment_policy_versions pv
        JOIN deployment_policies dp ON dp.id = pv.policy_id
        WHERE pv.publication_state = 'accepted'
          AND dp.current_published_version_id = pv.id
        ORDER BY pv.name
        "#,
    )
    .fetch_all(pool)
    .await
    .map_err(|e| anyhow::anyhow!("failed to fetch policy configurations: {}", e))?;

    // Filter to those whose config matches the requirement's technical enforcement.
    let candidates = rows
        .into_iter()
        .filter_map(|(policy_id, policy_version_id, policy_name, config)| {
            if technical_identity.is_implemented_by(&config) {
                Some(PolicyConfigRow {
                    policy_id,
                    policy_version_id,
                    policy_name,
                    config,
                })
            } else {
                None
            }
        })
        .collect();

    Ok(candidates)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn technical_identity_from_assertions_empty() {
        let assertions = vec![];
        let identity = RequirementTechnicalIdentity::from_assertions(assertions);
        assert!(identity.enforced_options.is_empty());
    }

    #[test]
    fn technical_identity_from_assertions_single_bool() {
        let assertions = vec![NixosOptionAssertionDraft {
            option_path: "services.openssh.enable".to_string(),
            expected_value: NixosLiteralValue::Boolean(false),
            nix_expression: "cfg.config.services.openssh.enable == false".to_string(),
            description: "Disable SSH".to_string(),
        }];

        let identity = RequirementTechnicalIdentity::from_assertions(assertions);
        assert_eq!(identity.enforced_options.len(), 1);
        assert_eq!(
            identity
                .enforced_options
                .get("services.openssh.enable")
                .unwrap(),
            &Value::Bool(false)
        );
    }

    #[test]
    fn technical_identity_from_assertions_multiple_options() {
        let assertions = vec![
            NixosOptionAssertionDraft {
                option_path: "services.openssh.enable".to_string(),
                expected_value: NixosLiteralValue::Boolean(false),
                nix_expression: "...".to_string(),
                description: "...".to_string(),
            },
            NixosOptionAssertionDraft {
                option_path: "services.openssh.settings.PermitRootLogin".to_string(),
                expected_value: NixosLiteralValue::StringLiteral("no".to_string()),
                nix_expression: "...".to_string(),
                description: "...".to_string(),
            },
            NixosOptionAssertionDraft {
                option_path: "services.openssh.settings.MaxAuthTries".to_string(),
                expected_value: NixosLiteralValue::Integer(3),
                nix_expression: "...".to_string(),
                description: "...".to_string(),
            },
        ];

        let identity = RequirementTechnicalIdentity::from_assertions(assertions);
        assert_eq!(identity.enforced_options.len(), 3);
        assert_eq!(
            identity
                .enforced_options
                .get("services.openssh.enable")
                .unwrap(),
            &Value::Bool(false)
        );
        assert_eq!(
            identity
                .enforced_options
                .get("services.openssh.settings.PermitRootLogin")
                .unwrap(),
            &Value::String("no".to_string())
        );
        assert_eq!(
            identity
                .enforced_options
                .get("services.openssh.settings.MaxAuthTries")
                .unwrap(),
            &Value::Number(3.into())
        );
    }

    #[test]
    fn is_implemented_by_exact_match() {
        let assertions = vec![NixosOptionAssertionDraft {
            option_path: "services.openssh.enable".to_string(),
            expected_value: NixosLiteralValue::Boolean(false),
            nix_expression: "...".to_string(),
            description: "...".to_string(),
        }];

        let identity = RequirementTechnicalIdentity::from_assertions(assertions);

        let policy_config = json!({
            "services.openssh.enable": false
        });

        assert!(identity.is_implemented_by(&policy_config));
    }

    #[test]
    fn is_implemented_by_exact_match_with_extra_options() {
        let assertions = vec![NixosOptionAssertionDraft {
            option_path: "services.openssh.enable".to_string(),
            expected_value: NixosLiteralValue::Boolean(false),
            nix_expression: "...".to_string(),
            description: "...".to_string(),
        }];

        let identity = RequirementTechnicalIdentity::from_assertions(assertions);

        // Policy has the required option plus extra options.
        let policy_config = json!({
            "services.openssh.enable": false,
            "services.openssh.settings.PermitRootLogin": "no",
            "networking.firewall.enable": true
        });

        assert!(identity.is_implemented_by(&policy_config));
    }

    #[test]
    fn is_implemented_by_missing_option() {
        let assertions = vec![NixosOptionAssertionDraft {
            option_path: "services.openssh.enable".to_string(),
            expected_value: NixosLiteralValue::Boolean(false),
            nix_expression: "...".to_string(),
            description: "...".to_string(),
        }];

        let identity = RequirementTechnicalIdentity::from_assertions(assertions);

        let policy_config = json!({
            "services.openssh.settings.PermitRootLogin": "no"
        });

        assert!(!identity.is_implemented_by(&policy_config));
    }

    #[test]
    fn is_implemented_by_value_mismatch() {
        let assertions = vec![NixosOptionAssertionDraft {
            option_path: "services.openssh.enable".to_string(),
            expected_value: NixosLiteralValue::Boolean(false),
            nix_expression: "...".to_string(),
            description: "...".to_string(),
        }];

        let identity = RequirementTechnicalIdentity::from_assertions(assertions);

        let policy_config = json!({
            "services.openssh.enable": true  // Mismatch!
        });

        assert!(!identity.is_implemented_by(&policy_config));
    }

    #[test]
    fn is_implemented_by_multiple_options_all_match() {
        let assertions = vec![
            NixosOptionAssertionDraft {
                option_path: "services.openssh.enable".to_string(),
                expected_value: NixosLiteralValue::Boolean(false),
                nix_expression: "...".to_string(),
                description: "...".to_string(),
            },
            NixosOptionAssertionDraft {
                option_path: "services.openssh.settings.PermitRootLogin".to_string(),
                expected_value: NixosLiteralValue::StringLiteral("no".to_string()),
                nix_expression: "...".to_string(),
                description: "...".to_string(),
            },
        ];

        let identity = RequirementTechnicalIdentity::from_assertions(assertions);

        let policy_config = json!({
            "services.openssh.enable": false,
            "services.openssh.settings.PermitRootLogin": "no"
        });

        assert!(identity.is_implemented_by(&policy_config));
    }

    #[test]
    fn is_implemented_by_multiple_options_one_mismatch() {
        let assertions = vec![
            NixosOptionAssertionDraft {
                option_path: "services.openssh.enable".to_string(),
                expected_value: NixosLiteralValue::Boolean(false),
                nix_expression: "...".to_string(),
                description: "...".to_string(),
            },
            NixosOptionAssertionDraft {
                option_path: "services.openssh.settings.PermitRootLogin".to_string(),
                expected_value: NixosLiteralValue::StringLiteral("no".to_string()),
                nix_expression: "...".to_string(),
                description: "...".to_string(),
            },
        ];

        let identity = RequirementTechnicalIdentity::from_assertions(assertions);

        let policy_config = json!({
            "services.openssh.enable": false,
            "services.openssh.settings.PermitRootLogin": "prohibit-password"  // Mismatch!
        });

        assert!(!identity.is_implemented_by(&policy_config));
    }

    #[test]
    fn is_implemented_by_non_object_config() {
        let assertions = vec![NixosOptionAssertionDraft {
            option_path: "services.openssh.enable".to_string(),
            expected_value: NixosLiteralValue::Boolean(false),
            nix_expression: "...".to_string(),
            description: "...".to_string(),
        }];

        let identity = RequirementTechnicalIdentity::from_assertions(assertions);

        // Config is not a JSON object.
        let policy_config = Value::String("invalid".to_string());

        assert!(!identity.is_implemented_by(&policy_config));
    }

    #[test]
    fn is_implemented_by_empty_identity_returns_false() {
        let identity = RequirementTechnicalIdentity::from_assertions(vec![]);

        let policy_config = json!({
            "services.openssh.enable": false
        });

        // Empty identity (no enforcement inferred) should not match anything.
        assert!(!identity.is_implemented_by(&policy_config));
    }

    #[test]
    fn description_empty() {
        let identity = RequirementTechnicalIdentity::from_assertions(vec![]);
        assert_eq!(identity.description(), "(no technical enforcement inferred)");
    }

    #[test]
    fn description_single_option() {
        let assertions = vec![NixosOptionAssertionDraft {
            option_path: "services.openssh.enable".to_string(),
            expected_value: NixosLiteralValue::Boolean(false),
            nix_expression: "...".to_string(),
            description: "...".to_string(),
        }];

        let identity = RequirementTechnicalIdentity::from_assertions(assertions);
        let desc = identity.description();
        assert!(desc.contains("services.openssh.enable"));
        assert!(desc.contains("false"));
    }

    #[test]
    fn description_multiple_options() {
        let assertions = vec![
            NixosOptionAssertionDraft {
                option_path: "services.openssh.enable".to_string(),
                expected_value: NixosLiteralValue::Boolean(false),
                nix_expression: "...".to_string(),
                description: "...".to_string(),
            },
            NixosOptionAssertionDraft {
                option_path: "services.openssh.settings.PermitRootLogin".to_string(),
                expected_value: NixosLiteralValue::StringLiteral("no".to_string()),
                nix_expression: "...".to_string(),
                description: "...".to_string(),
            },
        ];

        let identity = RequirementTechnicalIdentity::from_assertions(assertions);
        let desc = identity.description();
        assert!(desc.contains("services.openssh.enable"));
        assert!(desc.contains("PermitRootLogin"));
        assert!(desc.contains("no"));
    }
}
