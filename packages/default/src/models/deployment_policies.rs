use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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
    /// Custom Nix expression evaluation
    CustomCheck {
        /// Nix expression that should evaluate to true
        /// Available bindings: cfg (the nixosConfiguration)
        expression: String,
        /// Human-readable description
        description: String,
        /// Field name in the output JSON
        field_name: String,
        strict: bool,
    },
}

impl DeploymentPolicy {
    pub fn is_strict(&self) -> bool {
        match self {
            DeploymentPolicy::RequireCrystalForgeAgent { strict }
            | DeploymentPolicy::RequirePackages { strict, .. }
            | DeploymentPolicy::CustomCheck { strict, .. } => *strict,
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
            DeploymentPolicy::CustomCheck { description, .. } => description.clone(),
        }
    }

    /// Generate the Nix expression fragment for this policy
    /// Returns (field_name, nix_expression)
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
                    .map(|p| format!("\"{}\"", p.replace('"', "\\\""))) // Escape quotes
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
                ..
            } => (field_name.clone(), expression.clone()),
        }
    }

    /// Get the field name this policy uses in JSON output
    pub fn field_name(&self) -> String {
        match self {
            DeploymentPolicy::RequireCrystalForgeAgent { .. } => "cfAgentEnabled".to_string(),
            DeploymentPolicy::RequirePackages { .. } => "hasRequiredPackages".to_string(),
            DeploymentPolicy::CustomCheck { field_name, .. } => field_name.clone(),
        }
    }
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
}

impl PolicyCheckResult {
    /// Create a new PolicyCheckResult from parsed JSON and policies
    pub fn from_json(
        system_name: String,
        policies_json: &serde_json::Value,
        policies: &[DeploymentPolicy],
    ) -> Self {
        let mut warnings = Vec::new();
        let mut cf_agent_enabled = None;
        let mut has_required_packages = None;
        let mut custom_checks = HashMap::new();

        for policy in policies {
            let field_name = policy.field_name();
            let value = policies_json.get(&field_name).and_then(|v| v.as_bool());

            match policy {
                DeploymentPolicy::RequireCrystalForgeAgent { .. } => {
                    cf_agent_enabled = value;
                    if value != Some(true) {
                        warnings.push(format!(
                            "Crystal Forge agent not enabled for {}",
                            system_name
                        ));
                    }
                }
                DeploymentPolicy::RequirePackages { packages, .. } => {
                    has_required_packages = value;
                    if value != Some(true) {
                        warnings.push(format!(
                            "Missing required packages for {}: {}",
                            system_name,
                            packages.join(", ")
                        ));
                    }
                }
                DeploymentPolicy::CustomCheck {
                    description,
                    field_name,
                    ..
                } => {
                    if let Some(v) = value {
                        custom_checks.insert(field_name.clone(), v);
                        if !v {
                            warnings.push(format!("{}: {}", system_name, description));
                        }
                    } else {
                        warnings.push(format!(
                            "{}: Could not evaluate custom check '{}'",
                            system_name, description
                        ));
                    }
                }
            }
        }

        let meets_requirements = warnings.is_empty();

        PolicyCheckResult {
            system_name,
            cf_agent_enabled,
            has_required_packages,
            custom_checks,
            meets_requirements,
            warnings,
        }
    }
}

/// Build the complete Nix expression for nix-eval-jobs with policy checks
pub fn build_nix_eval_expression(flake_ref: &str, policies: &[DeploymentPolicy]) -> String {
    let policy_fields = if policies.is_empty() {
        // No policies - empty attrset
        "      # No policies configured".to_string()
    } else {
        policies
            .iter()
            .map(|policy| {
                let (field_name, expr) = policy.to_nix_expression();
                format!("      {} = {};", field_name, expr)
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
  builtins.mapAttrs (name: cfg: {{
    # Standard derivation info
    inherit name;
    drvPath = cfg.config.system.build.toplevel.drvPath or null;
    outputs = cfg.config.system.build.toplevel.outputs or {{}};
    
    # Policy check results (evaluated in parallel by nix-eval-jobs!)
    policies = {{
{}
    }};
  }}) configs
"#,
        flake_ref, policy_fields
    )
}

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
}
