use serde::{Deserialize, Serialize};
use sqlx::types::chrono::{DateTime, Utc};
use std::collections::HashMap;
use uuid::Uuid;

// ============================================================================
// CVE Check Config
// ============================================================================

/// Behaviour when no CVE scan has been completed for the derivation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WhenNoScan {
    /// Treat missing scan as a policy violation (default for strict policies).
    Block,
    /// Skip the check and allow deployment (useful during scan roll-out).
    Skip,
}

impl Default for WhenNoScan {
    fn default() -> Self {
        WhenNoScan::Block
    }
}

/// Configuration for a `require_cve_check` deployment policy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CveCheckConfig {
    /// Maximum allowed critical CVEs (0 = none allowed). Default 0.
    #[serde(default)]
    pub max_critical: u32,
    /// Maximum allowed high CVEs. None = no limit.
    #[serde(default)]
    pub max_high: Option<u32>,
    /// All high CVEs must have a whitelist_reason set. Default false.
    #[serde(default)]
    pub require_high_justification: bool,
    /// If true, violations block deployment. If false, only warn. Default true.
    #[serde(default = "default_strict")]
    pub strict: bool,
    /// What to do when no scan exists for the derivation.
    #[serde(default)]
    pub when_no_scan: WhenNoScan,
}

fn default_strict() -> bool {
    true
}

impl Default for CveCheckConfig {
    fn default() -> Self {
        CveCheckConfig {
            max_critical: 0,
            max_high: None,
            require_high_justification: false,
            strict: true,
            when_no_scan: WhenNoScan::Block,
        }
    }
}

// ============================================================================
// Multi-rule CustomCheck support
// ============================================================================

/// Mode for evaluating multiple rules.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleMode {
    /// All rules must pass.
    All,
    /// At least one rule must pass.
    Any,
}

impl Default for RuleMode {
    fn default() -> Self {
        RuleMode::All
    }
}

/// A single rule within a multi-rule custom_check policy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyRule {
    /// Nix expression for this rule.
    pub expression: String,
    /// Human-readable description.
    pub description: String,
    /// Field name in JSON output (must be unique within the policy).
    pub field_name: String,
    /// If true, this individual rule violation blocks deployment.
    #[serde(default = "default_strict")]
    pub strict: bool,
}

// ============================================================================
// DeploymentPolicy enum
// ============================================================================

/// A deployment policy that systems must satisfy
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DeploymentPolicy {
    /// Require Crystal Forge agent to be enabled
    RequireCrystalForgeAgent {
        /// If true, fail evaluation if agent is not enabled
        /// If false, just log a warning
        strict: bool,
    },
    /// Require specific packages to be installed
    RequirePackages { packages: Vec<String>, strict: bool },
    /// Custom Nix expression evaluation — single expression (legacy) or multi-rule.
    CustomCheck {
        /// Single Nix expression (backward-compat). Used when `rules` is empty.
        #[serde(default)]
        expression: String,
        /// Human-readable description (single-expression mode)
        #[serde(default)]
        description: String,
        /// Field name in the output JSON (single-expression mode)
        #[serde(default)]
        field_name: String,
        strict: bool,
        /// Multi-rule extension. When non-empty, `expression`/`description`/`field_name`
        /// at the top level are ignored and each rule is evaluated independently.
        #[serde(default)]
        rules: Vec<PolicyRule>,
        /// Evaluation mode for multi-rule policies.
        #[serde(default)]
        mode: RuleMode,
    },
    /// CVE-count gate evaluated against the database after build-complete.
    /// This policy type is NOT Nix-evaluated; it runs in the deployment manager.
    RequireCveCheck { config: CveCheckConfig },
}

impl DeploymentPolicy {
    pub fn is_strict(&self) -> bool {
        match self {
            DeploymentPolicy::RequireCrystalForgeAgent { strict }
            | DeploymentPolicy::RequirePackages { strict, .. }
            | DeploymentPolicy::CustomCheck { strict, .. } => *strict,
            DeploymentPolicy::RequireCveCheck { config } => config.strict,
        }
    }

    pub fn description(&self) -> String {
        match self {
            DeploymentPolicy::RequireCrystalForgeAgent { .. } => {
                "Crystal Forge agent must be enabled".to_string()
            }
            DeploymentPolicy::RequirePackages { packages, .. } => {
                format!("Required packages: {}", packages.join(", "))
            }
            DeploymentPolicy::CustomCheck {
                description, rules, ..
            } => {
                if rules.is_empty() {
                    description.clone()
                } else {
                    format!("Multi-rule check ({} rules)", rules.len())
                }
            }
            DeploymentPolicy::RequireCveCheck { config } => {
                let mut parts = Vec::new();
                parts.push(format!("max_critical={}", config.max_critical));
                if let Some(mh) = config.max_high {
                    parts.push(format!("max_high={}", mh));
                }
                if config.require_high_justification {
                    parts.push("require_high_justification".to_string());
                }
                format!("CVE gate: {}", parts.join(", "))
            }
        }
    }

    /// Returns true if this policy is evaluated via Nix (nix-eval-jobs path).
    /// RequireCveCheck is DB-evaluated and must be excluded from the Nix expression.
    pub fn is_nix_evaluated(&self) -> bool {
        !matches!(self, DeploymentPolicy::RequireCveCheck { .. })
    }

    /// Generate the Nix expression fragment for this policy.
    /// Returns (field_name, nix_expression).
    /// Panics if called on RequireCveCheck (use is_nix_evaluated() to guard).
    pub fn to_nix_expression(&self) -> (String, String) {
        match self {
            DeploymentPolicy::RequireCrystalForgeAgent { .. } => (
                "cfAgentEnabled".to_string(),
                "(cfg.config.services.crystal-forge.enable or false) && \
                 (cfg.config.services.crystal-forge.client.enable or false)"
                    .to_string(),
            ),
            DeploymentPolicy::RequirePackages { packages, .. } => {
                let package_list = packages
                    .iter()
                    .map(|p| format!("\"{}\"", p.replace('"', "\\\"")))
                    .collect::<Vec<_>>()
                    .join(" ");
                (
                    "hasRequiredPackages".to_string(),
                    format!(
                        "let pkgNames = builtins.map (p: p.pname or p.name or \"\") \
                         cfg.config.environment.systemPackages; \
                         required = [ {} ]; \
                         in builtins.all (pkg: builtins.elem pkg pkgNames) required",
                        package_list
                    ),
                )
            }
            DeploymentPolicy::CustomCheck {
                expression,
                field_name,
                rules,
                ..
            } => {
                if rules.is_empty() {
                    (field_name.clone(), expression.clone())
                } else {
                    // Multi-rule: return the first rule's expression as a placeholder;
                    // multi-rule nix expression building is handled in build_nix_eval_expression.
                    (rules[0].field_name.clone(), rules[0].expression.clone())
                }
            }
            DeploymentPolicy::RequireCveCheck { .. } => {
                panic!("RequireCveCheck is not a Nix-evaluated policy")
            }
        }
    }

    /// Get the field name this policy uses in JSON output
    pub fn field_name(&self) -> String {
        match self {
            DeploymentPolicy::RequireCrystalForgeAgent { .. } => "cfAgentEnabled".to_string(),
            DeploymentPolicy::RequirePackages { .. } => "hasRequiredPackages".to_string(),
            DeploymentPolicy::CustomCheck {
                field_name, rules, ..
            } => {
                if rules.is_empty() {
                    field_name.clone()
                } else {
                    // Multi-rule: field_name is not meaningful at the top level
                    format!("multiRule_{}", field_name)
                }
            }
            DeploymentPolicy::RequireCveCheck { .. } => "cveCheck".to_string(),
        }
    }
}

// ============================================================================
// PolicyCheckResult
// ============================================================================

/// Result of evaluating a single CVE policy against a derivation.
#[derive(Debug, Clone)]
pub struct CveCheckOutcome {
    pub policy_description: String,
    pub passed: bool,
    pub blocking: bool,
    pub reason: Option<String>,
}

/// Results from checking deployment policies for a single system
#[derive(Debug, Clone)]
pub struct PolicyCheckResult {
    pub system_name: String,
    pub cf_agent_enabled: Option<bool>,
    pub has_required_packages: Option<bool>,
    pub custom_checks: HashMap<String, bool>,
    pub meets_requirements: bool,
    pub warnings: Vec<String>,
    /// Tracks which policies failed (description, is_strict)
    pub failed_policies: Vec<(String, bool)>,
    /// CVE gate outcomes (populated after DB evaluation)
    pub cve_checks: Vec<CveCheckOutcome>,
}

impl PolicyCheckResult {
    /// Create a new PolicyCheckResult from parsed JSON and policies (Nix-evaluated path).
    pub fn from_json(
        system_name: String,
        policies_json: &serde_json::Value,
        policies: &[DeploymentPolicy],
    ) -> Self {
        let mut warnings = Vec::new();
        let mut cf_agent_enabled = None;
        let mut has_required_packages = None;
        let mut custom_checks = HashMap::new();
        let mut failed_policies = Vec::new();

        for policy in policies {
            // CVE policies are not Nix-evaluated; skip here.
            if !policy.is_nix_evaluated() {
                continue;
            }

            let is_strict = policy.is_strict();

            match policy {
                DeploymentPolicy::RequireCrystalForgeAgent { .. } => {
                    let field_name = policy.field_name();
                    let value = policies_json.get(&field_name).and_then(|v| v.as_bool());
                    cf_agent_enabled = value;
                    if value != Some(true) {
                        let desc = policy.description();
                        warnings.push(format!(
                            "Crystal Forge agent not enabled for {}",
                            system_name
                        ));
                        failed_policies.push((desc, is_strict));
                    }
                }
                DeploymentPolicy::RequirePackages { packages, .. } => {
                    let field_name = policy.field_name();
                    let value = policies_json.get(&field_name).and_then(|v| v.as_bool());
                    has_required_packages = value;
                    if value != Some(true) {
                        let desc = policy.description();
                        warnings.push(format!(
                            "Missing required packages for {}: {}",
                            system_name,
                            packages.join(", ")
                        ));
                        failed_policies.push((desc, is_strict));
                    }
                }
                DeploymentPolicy::CustomCheck {
                    description,
                    field_name,
                    rules,
                    mode,
                    strict,
                    ..
                } => {
                    if rules.is_empty() {
                        // Single-expression (legacy) path
                        let value = policies_json.get(field_name).and_then(|v| v.as_bool());
                        if let Some(v) = value {
                            custom_checks.insert(field_name.clone(), v);
                            if !v {
                                warnings.push(format!("{}: {}", system_name, description));
                                failed_policies.push((description.clone(), *strict));
                            }
                        } else {
                            warnings.push(format!(
                                "{}: Could not evaluate custom check '{}'",
                                system_name, description
                            ));
                            failed_policies.push((description.clone(), *strict));
                        }
                    } else {
                        // Multi-rule path.
                        //
                        // Step 1: collect raw results and per-rule warnings WITHOUT yet recording
                        // failures. We cannot know which per-rule failures matter until we know
                        // whether the overall mode verdict passes.
                        let mut rule_results: Vec<(bool, &PolicyRule)> = Vec::new();
                        for rule in rules {
                            let value = policies_json
                                .get(&rule.field_name)
                                .and_then(|v| v.as_bool());
                            let passed = value.unwrap_or(false);
                            custom_checks.insert(rule.field_name.clone(), passed);
                            rule_results.push((passed, rule));
                        }

                        // Step 2: compute overall mode verdict. This is authoritative.
                        let overall_passed = match mode {
                            RuleMode::All => rule_results.iter().all(|(p, _)| *p),
                            RuleMode::Any => rule_results.iter().any(|(p, _)| *p),
                        };

                        // Step 3: emit warnings and failures using the overall verdict.
                        //
                        // For Any: if at least one rule passed, the policy passes — no strict
                        // failures are recorded, even for failing alternatives. Individual
                        // warnings are still emitted for observability.
                        //
                        // For All: every failing rule contributes a strict failure if the
                        // rule is marked strict AND the policy as a whole failed.
                        for (passed, rule) in &rule_results {
                            if !passed {
                                warnings.push(format!(
                                    "{}: rule '{}' failed",
                                    system_name, rule.description
                                ));
                                // Only push strict failure when the overall policy failed.
                                // This ensures Any-mode does not punish failed alternatives
                                // when a sibling rule succeeded.
                                if !overall_passed && rule.strict {
                                    failed_policies.push((rule.description.clone(), true));
                                }
                            }
                        }

                        // Top-level policy failure when strict + overall failed.
                        if !overall_passed && *strict && failed_policies.is_empty() {
                            // No per-rule strict failures (all rules were non-strict) —
                            // record the overall policy failure at top-level.
                            failed_policies
                                .push((format!("Multi-rule check ({:?} mode) failed", mode), true));
                        }
                    }
                }
                DeploymentPolicy::RequireCveCheck { .. } => {
                    // Handled separately in check_cve_policies()
                }
            }
        }

        let meets_requirements = !failed_policies.iter().any(|(_, is_strict)| *is_strict);

        PolicyCheckResult {
            system_name,
            cf_agent_enabled,
            has_required_packages,
            custom_checks,
            meets_requirements,
            warnings,
            failed_policies,
            cve_checks: Vec::new(),
        }
    }
}

// ============================================================================
// Nix expression builder
// ============================================================================

/// Build the complete Nix expression for nix-eval-jobs with policy checks.
/// Only includes Nix-evaluated policies; CVE policies are excluded.
pub fn build_nix_eval_expression(flake_ref: &str, policies: &[DeploymentPolicy]) -> String {
    let nix_policies: Vec<&DeploymentPolicy> =
        policies.iter().filter(|p| p.is_nix_evaluated()).collect();

    let policy_fields = if nix_policies.is_empty() {
        "        # No policies configured".to_string()
    } else {
        nix_policies
            .iter()
            .flat_map(|policy| {
                match policy {
                    DeploymentPolicy::CustomCheck { rules, .. } if !rules.is_empty() => {
                        // Multi-rule: emit one field per rule
                        rules
                            .iter()
                            .map(|rule| {
                                format!("        {} = {};", rule.field_name, rule.expression)
                            })
                            .collect::<Vec<_>>()
                    }
                    _ => {
                        let (field_name, expr) = policy.to_nix_expression();
                        vec![format!("        {} = {};", field_name, expr)]
                    }
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    };

    format!(
        r#"
let
  flake = builtins.getFlake "{}";
  configs = flake.nixosConfigurations;
in
  builtins.mapAttrs (name: cfg: 
    let
      # The actual derivation that nix-eval-jobs expects
      drv = cfg.config.system.build.toplevel;
      
      # Policy check results
      policyResults = {{
{}
      }};
    in
      # Return the derivation WITH policy data attached as meta
      drv // {{
        # nix-eval-jobs will see this as a derivation and output it
        # We attach our policy data as an attribute
        meta = (drv.meta or {{}}) // {{
          policies = policyResults;
        }};
      }}
  ) configs
"#,
        flake_ref, policy_fields
    )
}

// ============================================================================
// Database Models for CRUD API
// ============================================================================

/// Database record for a deployment policy
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct DeploymentPolicyRecord {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    /// Policy type: 'require_cf_agent', 'require_packages', 'custom_check', 'require_cve_check'
    pub policy_type: String,
    /// JSON configuration specific to the policy type
    pub config: serde_json::Value,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Request to create a new deployment policy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateDeploymentPolicyRequest {
    pub name: String,
    pub description: Option<String>,
    pub policy_type: String,
    pub config: serde_json::Value,
    pub enabled: Option<bool>,
}

/// Request to update an existing deployment policy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateDeploymentPolicyRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub policy_type: Option<String>,
    pub config: Option<serde_json::Value>,
    pub enabled: Option<bool>,
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cf_agent_policy_expression() {
        let policy = DeploymentPolicy::RequireCrystalForgeAgent { strict: false };
        let (field_name, expr) = policy.to_nix_expression();
        assert_eq!(field_name, "cfAgentEnabled");
        assert!(expr.contains("services.crystal-forge.enable"));
        assert!(expr.contains("services.crystal-forge.client.enable"));
    }

    #[test]
    fn test_package_policy_expression() {
        let policy = DeploymentPolicy::RequirePackages {
            packages: vec!["vim".to_string(), "git".to_string()],
            strict: false,
        };
        let (field_name, expr) = policy.to_nix_expression();
        assert_eq!(field_name, "hasRequiredPackages");
        assert!(expr.contains("\"vim\""));
        assert!(expr.contains("\"git\""));
    }

    #[test]
    fn test_build_expression_no_policies() {
        let expr = build_nix_eval_expression("github:user/repo", &[]);
        assert!(expr.contains("builtins.getFlake"));
        assert!(expr.contains("No policies configured"));
    }

    #[test]
    fn test_build_expression_with_policies() {
        let policies = vec![
            DeploymentPolicy::RequireCrystalForgeAgent { strict: false },
            DeploymentPolicy::RequirePackages {
                packages: vec!["vim".to_string()],
                strict: false,
            },
        ];
        let expr = build_nix_eval_expression("github:user/repo", &policies);
        assert!(expr.contains("cfAgentEnabled"));
        assert!(expr.contains("hasRequiredPackages"));
        assert!(expr.contains("services.crystal-forge"));
    }

    #[test]
    fn cve_check_policy_excluded_from_nix_expression() {
        let policies = vec![
            DeploymentPolicy::RequireCrystalForgeAgent { strict: true },
            DeploymentPolicy::RequireCveCheck {
                config: CveCheckConfig::default(),
            },
        ];
        let expr = build_nix_eval_expression("github:user/repo", &policies);
        assert!(expr.contains("cfAgentEnabled"));
        assert!(
            !expr.contains("cveCheck"),
            "CVE policy must not appear in Nix expression"
        );
    }

    #[test]
    fn cve_check_config_default_values() {
        let config = CveCheckConfig::default();
        assert_eq!(config.max_critical, 0);
        assert!(config.max_high.is_none());
        assert!(!config.require_high_justification);
        assert!(config.strict);
        assert_eq!(config.when_no_scan, WhenNoScan::Block);
    }

    #[test]
    fn cve_check_config_round_trips_json() {
        let config = CveCheckConfig {
            max_critical: 2,
            max_high: Some(5),
            require_high_justification: true,
            strict: false,
            when_no_scan: WhenNoScan::Skip,
        };
        let json = serde_json::to_value(&config).unwrap();
        let back: CveCheckConfig = serde_json::from_value(json).unwrap();
        assert_eq!(back.max_critical, 2);
        assert_eq!(back.max_high, Some(5));
        assert!(back.require_high_justification);
        assert!(!back.strict);
        assert_eq!(back.when_no_scan, WhenNoScan::Skip);
    }

    #[test]
    fn multi_rule_custom_check_nix_expression_emits_all_rules() {
        let policies = vec![DeploymentPolicy::CustomCheck {
            expression: String::new(),
            description: String::new(),
            field_name: "parent".to_string(),
            strict: true,
            rules: vec![
                PolicyRule {
                    expression: "true".to_string(),
                    description: "rule a".to_string(),
                    field_name: "ruleA".to_string(),
                    strict: true,
                },
                PolicyRule {
                    expression: "false".to_string(),
                    description: "rule b".to_string(),
                    field_name: "ruleB".to_string(),
                    strict: false,
                },
            ],
            mode: RuleMode::All,
        }];
        let expr = build_nix_eval_expression("github:user/repo", &policies);
        assert!(expr.contains("ruleA"), "must emit ruleA field");
        assert!(expr.contains("ruleB"), "must emit ruleB field");
    }

    #[test]
    fn policy_check_result_multi_rule_all_mode_fails_on_any_failure() {
        let policies = vec![DeploymentPolicy::CustomCheck {
            expression: String::new(),
            description: "multi".to_string(),
            field_name: "multi".to_string(),
            strict: true,
            rules: vec![
                PolicyRule {
                    expression: "true".to_string(),
                    description: "passes".to_string(),
                    field_name: "ruleA".to_string(),
                    strict: true,
                },
                PolicyRule {
                    expression: "false".to_string(),
                    description: "fails".to_string(),
                    field_name: "ruleB".to_string(),
                    strict: true,
                },
            ],
            mode: RuleMode::All,
        }];
        let json = serde_json::json!({ "ruleA": true, "ruleB": false });
        let result = PolicyCheckResult::from_json("test-host".to_string(), &json, &policies);
        assert!(!result.meets_requirements);
        assert!(!result.failed_policies.is_empty());
    }

    #[test]
    fn policy_check_result_multi_rule_any_mode_passes_on_one_success() {
        // Any mode: ruleB passes → overall passes → ruleA's failure is not a strict failure.
        let policies = vec![DeploymentPolicy::CustomCheck {
            expression: String::new(),
            description: "multi-any".to_string(),
            field_name: "multi".to_string(),
            strict: true,
            rules: vec![
                PolicyRule {
                    expression: "false".to_string(),
                    description: "fails".to_string(),
                    field_name: "ruleA".to_string(),
                    strict: true, // strict on this rule — but overall Any passes, so no failure
                },
                PolicyRule {
                    expression: "true".to_string(),
                    description: "passes".to_string(),
                    field_name: "ruleB".to_string(),
                    strict: true,
                },
            ],
            mode: RuleMode::Any,
        }];
        let json = serde_json::json!({ "ruleA": false, "ruleB": true });
        let result = PolicyCheckResult::from_json("test-host".to_string(), &json, &policies);
        // Any mode: one rule passed → overall passed → no strict failures → meets_requirements
        assert!(result.meets_requirements);
        assert!(
            result.failed_policies.is_empty(),
            "Any-mode overall pass must not produce strict failures"
        );
    }

    #[test]
    fn policy_check_result_multi_rule_any_mode_fails_when_all_fail() {
        // Any mode: both fail → overall fails → per-rule strict failures are recorded.
        let policies = vec![DeploymentPolicy::CustomCheck {
            expression: String::new(),
            description: "multi-any-fail".to_string(),
            field_name: "multi".to_string(),
            strict: true,
            rules: vec![
                PolicyRule {
                    expression: "false".to_string(),
                    description: "ruleA fails".to_string(),
                    field_name: "ruleA".to_string(),
                    strict: true,
                },
                PolicyRule {
                    expression: "false".to_string(),
                    description: "ruleB fails".to_string(),
                    field_name: "ruleB".to_string(),
                    strict: true,
                },
            ],
            mode: RuleMode::Any,
        }];
        let json = serde_json::json!({ "ruleA": false, "ruleB": false });
        let result = PolicyCheckResult::from_json("test-host".to_string(), &json, &policies);
        assert!(!result.meets_requirements);
        assert!(!result.failed_policies.is_empty());
    }
}
