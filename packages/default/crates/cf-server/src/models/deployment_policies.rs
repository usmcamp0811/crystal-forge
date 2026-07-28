use serde::{Deserialize, Serialize};
use sqlx::types::chrono::{DateTime, Utc};
use std::collections::{BTreeMap, HashMap};
use tracing::warn;
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
// Time Window Config
// ============================================================================

/// Configuration for a `time_window` deployment policy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeWindowConfig {
    /// Human-readable description of this time window.
    pub description: String,
    /// Days of the week when deployment is allowed (e.g., ["mon", "tue", "wed", "thu", "fri"]).
    pub days: Vec<String>,
    /// Start time in HH:MM format (24-hour, e.g., "09:00").
    pub start_time: String,
    /// End time in HH:MM format (24-hour, e.g., "17:00").
    pub end_time: String,
    /// IANA timezone (e.g., "America/New_York").
    pub timezone: String,
    /// Action when outside window: "block" or "warn".
    #[serde(default = "default_action_block")]
    pub action: String,
}

fn default_action_block() -> String {
    "block".to_string()
}

// ============================================================================
// Approval Config
// ============================================================================

/// Configuration for a `require_approvals` deployment policy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalConfig {
    /// Human-readable description.
    pub description: String,
    /// Number of approvals required.
    pub count: u32,
    /// Role required for approvers (e.g., "admin", "operator").
    pub role: String,
    /// If true, approvers must be distinct users.
    #[serde(default = "default_true")]
    pub distinct: bool,
    /// Approval expiration in hours. None = never expires.
    #[serde(default)]
    pub expires_after_hours: Option<u32>,
}

fn default_true() -> bool {
    true
}

// ============================================================================
// Canary Rollout Config
// ============================================================================

/// Health check configuration for canary rollout.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckConfig {
    /// Health check type: "systemd", "custom_check", or "none".
    #[serde(rename = "type")]
    pub health_check_type: String,
    /// Number of system failures before halting rollout.
    #[serde(default)]
    pub fail_threshold: u32,
}

/// Configuration for a `canary_rollout` deployment policy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanaryConfig {
    /// Human-readable description.
    pub description: String,
    /// Percentage of fleet to deploy per phase (e.g., 25 = 25%).
    pub percentage: u32,
    /// Observation duration in minutes before proceeding to next phase.
    pub observe_duration_minutes: u32,
    /// System selection strategy: "random", "labeled", or "hash-based".
    pub selection_strategy: String,
    /// Health check configuration.
    pub health_check: HealthCheckConfig,
}

// ============================================================================
// CVE Threshold Config
// ============================================================================

/// Action for a specific severity level.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SeverityAction {
    /// Block deployment.
    Block,
    /// Log a warning but allow deployment.
    Warn,
}

/// Threshold configuration for a specific severity level.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeverityThreshold {
    /// Maximum allowed CVEs of this severity.
    pub max: u32,
    /// Action when threshold is exceeded.
    pub action: SeverityAction,
}

/// Configuration for a `cve_threshold` deployment policy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CveThresholdConfig {
    /// Human-readable description.
    pub description: String,
    /// Thresholds per severity level.
    pub thresholds: HashMap<String, SeverityThreshold>,
    /// What to do when no scan exists: "block", "skip", or "warn".
    #[serde(default = "default_no_scan_block")]
    pub no_scan_behavior: String,
    /// If true, allow operators to provide justifications for CVEs.
    #[serde(default)]
    pub allow_justifications: bool,
    /// If true, require acknowledgment even for warned CVEs.
    #[serde(default)]
    pub require_acknowledgment: bool,
}

fn default_no_scan_block() -> String {
    "block".to_string()
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

    /// Time-windowed deployment restriction.
    /// Deployment only allowed during specified time windows.
    /// NOT Nix-evaluated; checked at deployment time.
    TimeWindow { config: TimeWindowConfig },

    /// Multi-operator approval requirement.
    /// Requires N approvals from operators with specific roles.
    /// NOT Nix-evaluated; checked at deployment time.
    RequireApprovals { config: ApprovalConfig },

    /// Canary/phased rollout orchestration.
    /// Deploys to subsets of fleet with observation periods.
    /// NOT Nix-evaluated; controls deployment orchestration.
    CanaryRollout { config: CanaryConfig },

    /// Enhanced CVE threshold policy with per-severity actions.
    /// More flexible than RequireCveCheck.
    /// NOT Nix-evaluated; checked at deployment time.
    CveThreshold { config: CveThresholdConfig },
}

impl DeploymentPolicy {
    pub fn is_strict(&self) -> bool {
        match self {
            DeploymentPolicy::RequireCrystalForgeAgent { strict }
            | DeploymentPolicy::RequirePackages { strict, .. }
            | DeploymentPolicy::CustomCheck { strict, .. } => *strict,
            DeploymentPolicy::RequireCveCheck { config } => config.strict,
            // New policy types are always strict (they block deployment when conditions not met)
            DeploymentPolicy::TimeWindow { config } => config.action == "block",
            DeploymentPolicy::RequireApprovals { .. } => true,
            DeploymentPolicy::CanaryRollout { .. } => true,
            DeploymentPolicy::CveThreshold { .. } => true,
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
            DeploymentPolicy::TimeWindow { config } => config.description.clone(),
            DeploymentPolicy::RequireApprovals { config } => config.description.clone(),
            DeploymentPolicy::CanaryRollout { config } => config.description.clone(),
            DeploymentPolicy::CveThreshold { config } => config.description.clone(),
        }
    }

    /// Returns true if this policy is evaluated via Nix (nix-eval-jobs path).
    /// RequireCveCheck and new policy types are DB/deployment-time evaluated.
    pub fn is_nix_evaluated(&self) -> bool {
        !matches!(
            self,
            DeploymentPolicy::RequireCveCheck { .. }
                | DeploymentPolicy::TimeWindow { .. }
                | DeploymentPolicy::RequireApprovals { .. }
                | DeploymentPolicy::CanaryRollout { .. }
                | DeploymentPolicy::CveThreshold { .. }
        )
    }

    /// Generate the Nix expression fragment for this policy.
    /// Returns (field_name, nix_expression).
    /// Panics if called on RequireCveCheck (use is_nix_evaluated() to guard).
    pub fn to_nix_expression_with_index(&self, index: usize) -> (String, String) {
        match self {
            DeploymentPolicy::RequireCrystalForgeAgent { .. } => (
                "cfAgentEnabled".to_string(),
                "(cfg.config.systemd.services.crystal-forge-agent.enable or false) || \
                 ((cfg.config.services.crystal-forge.enable or false) && \
                  (cfg.config.services.crystal-forge.client.enable or false))"
                    .to_string(),
            ),
            DeploymentPolicy::RequirePackages { packages, .. } => {
                let package_list = packages
                    .iter()
                    .map(|p| format!("\"{}\"", p.replace('"', "\\\"")))
                    .collect::<Vec<_>>()
                    .join(" ");
                (
                    format!("hasRequiredPackages_{index}"),
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
            DeploymentPolicy::TimeWindow { .. } => {
                panic!("TimeWindow is not a Nix-evaluated policy")
            }
            DeploymentPolicy::RequireApprovals { .. } => {
                panic!("RequireApprovals is not a Nix-evaluated policy")
            }
            DeploymentPolicy::CanaryRollout { .. } => {
                panic!("CanaryRollout is not a Nix-evaluated policy")
            }
            DeploymentPolicy::CveThreshold { .. } => {
                panic!("CveThreshold is not a Nix-evaluated policy")
            }
        }
    }

    pub fn to_nix_expression(&self) -> (String, String) {
        self.to_nix_expression_with_index(0)
    }

    /// Get the field name this policy uses in JSON output
    pub fn field_name_with_index(&self, index: usize) -> String {
        match self {
            DeploymentPolicy::RequireCrystalForgeAgent { .. } => "cfAgentEnabled".to_string(),
            DeploymentPolicy::RequirePackages { .. } => format!("hasRequiredPackages_{index}"),
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
            DeploymentPolicy::TimeWindow { .. } => "timeWindow".to_string(),
            DeploymentPolicy::RequireApprovals { .. } => "requireApprovals".to_string(),
            DeploymentPolicy::CanaryRollout { .. } => "canaryRollout".to_string(),
            DeploymentPolicy::CveThreshold { .. } => "cveThreshold".to_string(),
        }
    }

    pub fn field_name(&self) -> String {
        self.field_name_with_index(0)
    }
}

// ============================================================================
// Per-configuration policy map
// ============================================================================

/// A deployment policy together with its stable database UUID, used when
/// policies are scoped to individual NixOS configurations rather than
/// applied flake-wide.
#[derive(Debug, Clone)]
pub struct AssignedPolicy {
    /// Stable database UUID for deterministic ordering and deduplication.
    pub policy_id: Uuid,
    /// The parsed deployment policy.
    pub policy: DeploymentPolicy,
}

/// Map from NixOS configuration name to the ordered, deduplicated list of
/// policies assigned to that configuration (via its environment or directly).
///
/// - Keys: `COALESCE(NULLIF(BTRIM(system_configuration_name), ''), hostname)`.
/// - Values: sorted by `policy_id`, deduplicated.
/// - Configurations with zero assigned policies produce **no** entry in the map;
///   use `policies_for_config(map, name)` to get an empty slice safely.
/// - Configurations not registered in Crystal Forge also produce no entry.
pub type PoliciesByConfiguration = BTreeMap<String, Vec<AssignedPolicy>>;

/// Nix result keys that are reserved for built-in evaluator metadata and may
/// not be used as custom-check `field_name` values. Overriding these from a
/// user-defined policy would let a policy spoof system-level safety signals.
pub const RESERVED_POLICY_RESULT_FIELDS: &[&str] = &["cfAgentEnabled"];

/// Returns true if `field_name` is reserved for built-in evaluator metadata.
pub fn is_reserved_policy_result_field(field_name: &str) -> bool {
    RESERVED_POLICY_RESULT_FIELDS
        .iter()
        .any(|reserved| reserved.eq_ignore_ascii_case(field_name))
}

fn policy_kind(policy: &DeploymentPolicy) -> &'static str {
    match policy {
        DeploymentPolicy::RequireCrystalForgeAgent { .. } => "require_cf_agent",
        DeploymentPolicy::RequirePackages { .. } => "require_packages",
        DeploymentPolicy::CustomCheck { .. } => "custom_check",
        DeploymentPolicy::RequireCveCheck { .. } => "require_cve_check",
        DeploymentPolicy::TimeWindow { .. } => "time_window",
        DeploymentPolicy::RequireApprovals { .. } => "require_approvals",
        DeploymentPolicy::CanaryRollout { .. } => "canary_rollout",
        DeploymentPolicy::CveThreshold { .. } => "cve_threshold",
    }
}

fn policy_result_detail(policy: &DeploymentPolicy, passed: Option<bool>) -> Option<String> {
    if passed != Some(false) {
        return None;
    }

    match policy {
        DeploymentPolicy::RequireCrystalForgeAgent { .. } => {
            Some("Crystal Forge agent is disabled".to_string())
        }
        DeploymentPolicy::RequirePackages { packages, .. } => Some(format!(
            "Missing required packages: {}",
            packages.join(", ")
        )),
        DeploymentPolicy::CustomCheck {
            description, rules, ..
        } if rules.is_empty() => Some(description.clone()),
        DeploymentPolicy::CustomCheck { rules, .. } => {
            let descriptions: Vec<&str> =
                rules.iter().map(|rule| rule.description.as_str()).collect();
            Some(format!(
                "Failed custom-check rules: {}",
                descriptions.join(", ")
            ))
        }
        _ => Some(policy.description()),
    }
}

fn assigned_policy_passed(policy: &DeploymentPolicy, check: &PolicyCheckResult) -> Option<bool> {
    match policy {
        DeploymentPolicy::RequireCrystalForgeAgent { .. } => check.cf_agent_enabled,
        DeploymentPolicy::RequirePackages { .. } => check.has_required_packages,
        DeploymentPolicy::CustomCheck {
            field_name,
            rules,
            mode,
            ..
        } if rules.is_empty() => check.custom_checks.get(field_name).copied(),
        DeploymentPolicy::CustomCheck { rules, mode, .. } => {
            let values: Vec<bool> = rules
                .iter()
                .filter_map(|rule| check.custom_checks.get(&rule.field_name).copied())
                .collect();
            if values.len() != rules.len() {
                return None;
            }
            Some(match mode {
                RuleMode::All => values.iter().all(|v| *v),
                RuleMode::Any => values.iter().any(|v| *v),
            })
        }
        _ => None,
    }
}

/// Build the persisted policy-result document for a successfully evaluated
/// NixOS configuration. This is the source of truth for queue counters and the
/// policy matrix; the legacy `cf_agent_enabled` column remains a fast global
/// signal and compatibility field.
pub fn policy_results_json(
    check: &PolicyCheckResult,
    assigned: &[AssignedPolicy],
) -> serde_json::Value {
    let mut assigned_results = serde_json::Map::new();

    for assigned_policy in assigned.iter().filter(|ap| ap.policy.is_nix_evaluated()) {
        let passed = assigned_policy_passed(&assigned_policy.policy, check);
        assigned_results.insert(
            assigned_policy.policy_id.to_string(),
            serde_json::json!({
                "name": assigned_policy.policy.description(),
                "type": policy_kind(&assigned_policy.policy),
                "strict": assigned_policy.policy.is_strict(),
                "passed": passed,
                "details": policy_result_detail(&assigned_policy.policy, passed),
            }),
        );
    }

    serde_json::json!({
        "global": {
            "cfAgentEnabled": {
                "passed": check.cf_agent_enabled,
                "strict": true,
                "details": if check.cf_agent_enabled == Some(false) {
                    Some("Crystal Forge agent is disabled")
                } else {
                    None::<&str>
                }
            }
        },
        "assigned": assigned_results,
    })
}

pub fn policy_requirements_met(check: &PolicyCheckResult) -> bool {
    check.cf_agent_enabled == Some(true) && !check.failed_policies.iter().any(|(_, strict)| *strict)
}

/// Look up the assigned policies for a NixOS configuration name, returning an
/// empty slice when the configuration is unregistered or has no policies.
pub fn policies_for_config<'a>(
    map: &'a PoliciesByConfiguration,
    configuration_name: &str,
) -> &'a [AssignedPolicy] {
    map.get(configuration_name)
        .map(Vec::as_slice)
        .unwrap_or(&[])
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
    /// Create a `PolicyCheckResult` from Nix-output JSON using the per-configuration
    /// `AssignedPolicy` list (stable-key path).
    ///
    /// Uses `policy_result_key(policy_id)` for CF-agent and package checks, and
    /// `rule.field_name` for multi-rule custom checks (existing convention).
    ///
    /// Returns `None` when a Nix-evaluated policy's expected key is absent from
    /// the JSON (indicating an expression-generation/parser mismatch rather than
    /// a normal policy failure).  The caller should treat `None` as an
    /// infrastructure error for that configuration.
    pub fn from_assigned(
        system_name: String,
        policies_json: &serde_json::Value,
        assigned: &[AssignedPolicy],
    ) -> Result<Self, String> {
        let mut warnings = Vec::new();
        let mut has_required_packages: Option<bool> = None;
        let mut custom_checks = HashMap::new();
        let mut failed_policies = Vec::new();

        // cfAgentEnabled is emitted unconditionally by the evaluator for every
        // configuration, even when no require_cf_agent policy is assigned. The
        // build-job insert predicate depends on this value being present and
        // boolean, so a missing or non-boolean key is treated as an
        // infrastructure/parser mismatch rather than silently defaulting to None
        // (which would drop builds).
        let mut cf_agent_enabled: Option<bool> = match policies_json.get("cfAgentEnabled") {
            Some(v) => Some(v.as_bool().ok_or_else(|| {
                format!(
                    "Configuration {:?}: metadata key \"cfAgentEnabled\" must be boolean, got {}",
                    system_name, v
                )
            })?),
            None => {
                return Err(format!(
                    "Configuration {:?}: expected unconditional metadata key \"cfAgentEnabled\" \
                     but it was absent (available: {:?})",
                    system_name,
                    policies_json
                        .as_object()
                        .map(|o| o.keys().collect::<Vec<_>>()),
                ));
            }
        };

        for (idx, ap) in assigned.iter().enumerate() {
            if !ap.policy.is_nix_evaluated() {
                continue;
            }
            let is_strict = ap.policy.is_strict();
            let key = policy_result_key(&ap.policy_id);

            match &ap.policy {
                DeploymentPolicy::RequireCrystalForgeAgent { .. } => {
                    let value = match policies_json.get(&key) {
                        Some(v) => v.as_bool().ok_or_else(|| {
                            format!(
                                "Configuration {:?}: metadata key {:?} for CF-agent policy \
                                 (id={}) must be boolean, got {}",
                                system_name, key, ap.policy_id, v
                            )
                        })?,
                        None => {
                            return Err(format!(
                                "Configuration {:?}: expected Nix metadata key {:?} for \
                                 CF-agent policy (id={}) but key was absent (available: {:?})",
                                system_name,
                                key,
                                ap.policy_id,
                                policies_json
                                    .as_object()
                                    .map(|o| o.keys().collect::<Vec<_>>()),
                            ));
                        }
                    };
                    cf_agent_enabled = Some(value);
                    if !value {
                        warnings.push(format!(
                            "Crystal Forge agent not enabled for {}",
                            system_name
                        ));
                        failed_policies.push((ap.policy.description(), is_strict));
                    }
                }
                DeploymentPolicy::RequirePackages { packages, .. } => {
                    let value = match policies_json.get(&key) {
                        Some(v) => v.as_bool().ok_or_else(|| {
                            format!(
                                "Configuration {:?}: metadata key {:?} for require_packages \
                                 policy (id={}) must be boolean, got {}",
                                system_name, key, ap.policy_id, v
                            )
                        })?,
                        None => {
                            return Err(format!(
                                "Configuration {:?}: expected Nix metadata key {:?} for \
                                 require_packages policy (id={}) but key was absent (available: {:?})",
                                system_name,
                                key,
                                ap.policy_id,
                                policies_json
                                    .as_object()
                                    .map(|o| o.keys().collect::<Vec<_>>()),
                            ));
                        }
                    };
                    has_required_packages = Some(value);
                    if !value {
                        warnings.push(format!(
                            "Missing required packages for {}: {}",
                            system_name,
                            packages.join(", ")
                        ));
                        failed_policies.push((ap.policy.description(), is_strict));
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
                        // Legacy single-expression: use field_name from config.
                        match policies_json.get(field_name) {
                            Some(v) => {
                                let v = v.as_bool().ok_or_else(|| {
                                    format!(
                                        "Configuration {:?}: custom check {:?} must evaluate \
                                         to boolean, got {}",
                                        system_name, field_name, v
                                    )
                                })?;
                                custom_checks.insert(field_name.clone(), v);
                                if !v {
                                    warnings.push(format!("{}: {}", system_name, description));
                                    failed_policies.push((description.clone(), *strict));
                                }
                            }
                            None => {
                                return Err(format!(
                                    "Configuration {:?}: custom check {:?} was absent from \
                                     evaluator output (available: {:?})",
                                    system_name,
                                    field_name,
                                    policies_json
                                        .as_object()
                                        .map(|o| o.keys().collect::<Vec<_>>()),
                                ));
                            }
                        }
                    } else {
                        // Multi-rule: use per-rule field_name (existing convention).
                        let mut rule_results: Vec<(bool, &PolicyRule)> = Vec::new();
                        for rule in rules {
                            let passed = match policies_json.get(&rule.field_name) {
                                Some(v) => v.as_bool().ok_or_else(|| {
                                    format!(
                                        "Configuration {:?}: custom-check rule {:?} must \
                                         evaluate to boolean, got {}",
                                        system_name, rule.field_name, v
                                    )
                                })?,
                                None => {
                                    return Err(format!(
                                        "Configuration {:?}: custom-check rule {:?} was absent \
                                         from evaluator output (available: {:?})",
                                        system_name,
                                        rule.field_name,
                                        policies_json
                                            .as_object()
                                            .map(|o| o.keys().collect::<Vec<_>>()),
                                    ));
                                }
                            };
                            custom_checks.insert(rule.field_name.clone(), passed);
                            rule_results.push((passed, rule));
                        }
                        let overall_passed = match mode {
                            RuleMode::All => rule_results.iter().all(|(p, _)| *p),
                            RuleMode::Any => rule_results.iter().any(|(p, _)| *p),
                        };
                        for (passed, rule) in &rule_results {
                            if !passed {
                                warnings.push(format!(
                                    "{}: rule '{}' failed",
                                    system_name, rule.description
                                ));
                                if !overall_passed && rule.strict {
                                    failed_policies.push((rule.description.clone(), true));
                                }
                            }
                        }
                        if !overall_passed && *strict && failed_policies.is_empty() {
                            failed_policies
                                .push((format!("Multi-rule check ({:?} mode) failed", mode), true));
                        }
                    }
                    let _ = (idx, key); // suppress unused warnings
                }
                DeploymentPolicy::RequireCveCheck { .. }
                | DeploymentPolicy::TimeWindow { .. }
                | DeploymentPolicy::RequireApprovals { .. }
                | DeploymentPolicy::CanaryRollout { .. }
                | DeploymentPolicy::CveThreshold { .. } => {
                    // Not Nix-evaluated; already filtered by is_nix_evaluated().
                    let _ = (idx, key);
                }
            }
        }

        let meets_requirements = !failed_policies.iter().any(|(_, is_strict)| *is_strict);
        Ok(PolicyCheckResult {
            system_name,
            cf_agent_enabled,
            has_required_packages,
            custom_checks,
            meets_requirements,
            warnings,
            failed_policies,
            cve_checks: Vec::new(),
        })
    }

    /// Create a new PolicyCheckResult from parsed JSON and policies (Nix-evaluated path).
    /// Legacy flat-policies path; kept for existing tests.
    pub fn from_json(
        system_name: String,
        policies_json: &serde_json::Value,
        policies: &[DeploymentPolicy],
    ) -> Self {
        let mut warnings = Vec::new();
        // Always read cfAgentEnabled when present. The evaluator emits this
        // metadata even when require_cf_agent is not configured as an active
        // deployment policy, because build-job eligibility still depends on
        // whether the target can run the Crystal Forge agent.
        let mut cf_agent_enabled = policies_json
            .get("cfAgentEnabled")
            .and_then(|v| v.as_bool());
        let mut has_required_packages = None;
        let mut custom_checks = HashMap::new();
        let mut failed_policies = Vec::new();

        let mut nix_policy_idx = 0usize;
        for policy in policies {
            // CVE policies are not Nix-evaluated; skip here.
            if !policy.is_nix_evaluated() {
                continue;
            }

            let policy_idx = nix_policy_idx;
            nix_policy_idx += 1;

            let is_strict = policy.is_strict();

            match policy {
                DeploymentPolicy::RequireCrystalForgeAgent { .. } => {
                    let field_name = policy.field_name_with_index(policy_idx);
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
                    let field_name = policy.field_name_with_index(policy_idx);
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
                DeploymentPolicy::TimeWindow { .. } => {
                    // Deployment-time policy, not Nix-evaluated
                }
                DeploymentPolicy::RequireApprovals { .. } => {
                    // Deployment-time policy, not Nix-evaluated
                }
                DeploymentPolicy::CanaryRollout { .. } => {
                    // Deployment-time policy, not Nix-evaluated
                }
                DeploymentPolicy::CveThreshold { .. } => {
                    // Deployment-time policy, not Nix-evaluated
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

/// Safely produce a quoted Nix string literal from a Rust string.
/// JSON string encoding is a strict superset of Nix quoted-string encoding
/// for ordinary Unicode text, so this produces a correct Nix literal.
pub fn nix_string_pub(value: &str) -> String {
    nix_string(value)
}

fn nix_string(value: &str) -> String {
    // serde_json::to_string always succeeds for strings.
    serde_json::to_string(value).unwrap_or_else(|_| format!("\"{}\"", value.replace('"', "\\\"")))
}

/// Return the stable metadata key used in the Nix expression and parsed in
/// `PolicyCheckResult::from_json` for an assigned policy with the given UUID.
pub fn policy_result_key(policy_id: &Uuid) -> String {
    // Use the first 8 hex chars of the UUID to keep keys readable and unique.
    format!("policy_{}", policy_id.to_string().replace('-', ""))
}

/// Build the Nix field lines for one configuration's assigned policies (standalone path).
/// Returns a vec of `"  key = expr;"` strings (2-space indent for standalone expression).
pub fn build_policy_fields_for_config_standalone(assigned: &[AssignedPolicy]) -> Vec<String> {
    build_policy_fields_for_config_indented(assigned, "  ")
}

/// Build the Nix field lines for one configuration's assigned policies.
/// Returns a vec of `"        key = expr;"` strings.
fn build_policy_fields_for_config(assigned: &[AssignedPolicy]) -> Vec<String> {
    build_policy_fields_for_config_indented(assigned, "            ")
}

fn build_policy_fields_for_config_indented(
    assigned: &[AssignedPolicy],
    indent: &str,
) -> Vec<String> {
    let mut lines = Vec::new();

    for (idx, ap) in assigned.iter().enumerate() {
        if !ap.policy.is_nix_evaluated() {
            continue;
        }
        let key = policy_result_key(&ap.policy_id);
        match &ap.policy {
            DeploymentPolicy::CustomCheck {
                expression,
                field_name,
                rules,
                ..
            } if rules.is_empty() => {
                // Legacy single-expression custom check: emit under the configured
                // field_name so the parser can find it. The expression is inserted
                // verbatim and must use the `cfg.config.*` lexical contract.
                if is_reserved_policy_result_field(field_name) {
                    warn!(
                        policy_id = %ap.policy_id,
                        field_name = %field_name,
                        "Skipping custom check with reserved result field name"
                    );
                    continue;
                }
                lines.push(format!(
                    "{}{} = {};",
                    indent,
                    nix_string(field_name),
                    expression
                ));
            }
            DeploymentPolicy::CustomCheck { rules, .. } => {
                // Multi-rule: emit one line per rule using the rule's own field_name
                // (existing convention; rules predate stable-ID keys).
                // Expressions are expected to use `cfg.config.*` per the documented
                // policy fragment lexical contract.
                for rule in rules {
                    if is_reserved_policy_result_field(&rule.field_name) {
                        warn!(
                            policy_id = %ap.policy_id,
                            field_name = %rule.field_name,
                            "Skipping custom-check rule with reserved result field name"
                        );
                        continue;
                    }
                    lines.push(format!(
                        "{}{} = {};",
                        indent,
                        nix_string(&rule.field_name),
                        rule.expression
                    ));
                }
            }
            _ => {
                // All built-in Nix-evaluated policies (require_cf_agent,
                // require_packages) use `cfg.config.*` in their expression
                // fragments because the checker function receives the full
                // nixosConfigurations.<name> object as `cfg`.
                let (_, expr) = ap.policy.to_nix_expression_with_index(idx);
                lines.push(format!("{}{} = {};", indent, key, expr));
            }
        }
    }

    lines
}

/// Build the complete Nix expression for `nix-eval-jobs` with per-configuration
/// policy checks derived from the `PoliciesByConfiguration` map.
///
/// Each `nixosConfigurations.<name>` output is checked only against the
/// policies assigned to the Crystal Forge system(s) for that configuration.
/// Configurations that are unregistered or have no assigned policies receive
/// only the unconditional `cfAgentEnabled` metadata.
///
/// The expression structure:
/// ```nix
/// let
///   flake = builtins.getFlake "<flakeRef>";
///   policyCheckers = {
///     "<config>" = cfg: { policy_<id> = <expr>; ... };
///     ...
///   };
/// in builtins.mapAttrs (name: cfg:
///   let
///     drv = cfg.config.system.build.toplevel;
///     checker = policyCheckers.${name} or (_: {});
///   in drv // { meta = (drv.meta or {}) // { policies = (checker cfg) // { cfAgentEnabled = cfAgentEnabledExpr cfg; }; }; }
/// ) flake.nixosConfigurations
/// ```
pub fn build_nix_eval_expression(
    flake_ref: &str,
    policies_by_configuration: &PoliciesByConfiguration,
) -> String {
    // Build per-configuration checker blocks.
    let checker_entries: Vec<String> = policies_by_configuration
        .iter()
        .map(|(config_name, assigned)| {
            let field_lines = build_policy_fields_for_config(assigned);
            let body = if field_lines.is_empty() {
                "{}".to_string()
            } else {
                format!("{{\n{}\n          }}", field_lines.join("\n"))
            };
            format!("        {} = cfg: {};", nix_string(config_name), body)
        })
        .collect();

    let checkers_block = if checker_entries.is_empty() {
        "{}".to_string()
    } else {
        format!("{{\n{}\n      }}", checker_entries.join("\n"))
    };

    format!(
        r#"
let
  flake = builtins.getFlake {flake_ref};
  policyCheckers = {checkers};
  cfAgentEnabledExpr = cfg:
    (cfg.config.systemd.services.crystal-forge-agent.enable or false)
    || ((cfg.config.services.crystal-forge.enable or false)
        && (cfg.config.services.crystal-forge.client.enable or false));
in
  builtins.mapAttrs (name: cfg:
    let
      drv     = cfg.config.system.build.toplevel;
      checker = policyCheckers.${{name}} or (_: {{}});
    in
      drv // {{
        meta = (drv.meta or {{}}) // {{
          policies = (checker cfg) // {{ cfAgentEnabled = cfAgentEnabledExpr cfg; }};
        }};
      }}
  ) flake.nixosConfigurations
"#,
        flake_ref = nix_string(flake_ref),
        checkers = checkers_block,
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

    /// Test helper: wrap a flat `Vec<DeploymentPolicy>` into a `PoliciesByConfiguration`
    /// under the key `"test-config"` with sequential UUIDs.
    fn policies_map_for(policies: Vec<DeploymentPolicy>) -> PoliciesByConfiguration {
        let mut map = PoliciesByConfiguration::new();
        let assigned: Vec<AssignedPolicy> = policies
            .into_iter()
            .enumerate()
            .map(|(i, policy)| AssignedPolicy {
                policy_id: uuid::Uuid::from_u128(i as u128 + 1),
                policy,
            })
            .collect();
        if !assigned.is_empty() {
            map.insert("test-config".to_string(), assigned);
        }
        map
    }

    #[test]
    fn test_cf_agent_policy_expression() {
        let policy = DeploymentPolicy::RequireCrystalForgeAgent { strict: false };
        let (field_name, expr) = policy.to_nix_expression();
        assert_eq!(field_name, "cfAgentEnabled");
        assert!(expr.contains("systemd.services.crystal-forge-agent.enable"));
        assert!(expr.contains("services.crystal-forge.enable"));
        assert!(expr.contains("services.crystal-forge.client.enable"));
    }

    #[test]
    fn crystal_forge_agent_policy_expression_checks_systemd_service() {
        let policy = DeploymentPolicy::RequireCrystalForgeAgent { strict: true };
        let (_, expr) = policy.to_nix_expression();

        assert!(
            expr.starts_with("(cfg.config.systemd.services.crystal-forge-agent.enable or false)"),
            "cf agent policy should use the realized systemd service as the primary signal"
        );
        assert!(
            expr.contains("||"),
            "cf agent policy should retain compatibility with legacy service options"
        );
    }

    #[test]
    fn test_package_policy_expression() {
        let policy = DeploymentPolicy::RequirePackages {
            packages: vec!["vim".to_string(), "git".to_string()],
            strict: false,
        };
        let (field_name, expr) = policy.to_nix_expression_with_index(2);
        assert_eq!(field_name, "hasRequiredPackages_2");
        assert!(expr.contains("\"vim\""));
        assert!(expr.contains("\"git\""));
    }

    #[test]
    fn test_build_expression_no_policies() {
        let map = PoliciesByConfiguration::new();
        let expr = build_nix_eval_expression("github:user/repo", &map);
        assert!(expr.contains("builtins.getFlake"));
        // Empty map → no configuration-specific checkers, but cfAgentEnabled
        // must still be emitted unconditionally so builds can be queued.
        assert!(expr.contains("policyCheckers"));
        assert!(expr.contains("cfAgentEnabled"));
        assert!(expr.contains("cfAgentEnabledExpr"));
    }

    #[test]
    fn policy_check_result_reads_cf_agent_metadata_without_policy() {
        let policies_json = serde_json::json!({
            "cfAgentEnabled": true,
        });

        // Legacy from_json still works with empty slice.
        let result = PolicyCheckResult::from_json("host-a".to_string(), &policies_json, &[]);

        assert_eq!(result.cf_agent_enabled, Some(true));
        assert!(result.meets_requirements);
        assert!(result.failed_policies.is_empty());
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
        let map = policies_map_for(policies);
        let expr = build_nix_eval_expression("github:user/repo", &map);
        // Stable keys: policy_00000000000000000000000000000001 and _2
        assert!(expr.contains("crystal-forge-agent.enable"));
        assert!(expr.contains("test-config"));
        assert!(expr.contains("policyCheckers"));
    }

    #[test]
    fn require_packages_policies_get_distinct_field_names() {
        let id1 = uuid::Uuid::from_u128(1);
        let id2 = uuid::Uuid::from_u128(2);
        let key1 = policy_result_key(&id1);
        let key2 = policy_result_key(&id2);

        let mut map = PoliciesByConfiguration::new();
        map.insert(
            "test-config".to_string(),
            vec![
                AssignedPolicy {
                    policy_id: id1,
                    policy: DeploymentPolicy::RequirePackages {
                        packages: vec!["grafana".to_string()],
                        strict: true,
                    },
                },
                AssignedPolicy {
                    policy_id: id2,
                    policy: DeploymentPolicy::RequirePackages {
                        packages: vec!["neovim".to_string()],
                        strict: true,
                    },
                },
            ],
        );

        let expr = build_nix_eval_expression("github:user/repo", &map);
        assert!(expr.contains(&key1), "must contain key for policy id1");
        assert!(expr.contains(&key2), "must contain key for policy id2");
        assert_ne!(key1, key2, "keys must be distinct");
    }

    #[test]
    fn require_packages_indices_ignore_non_nix_policies_consistently() {
        // from_json (legacy path) still works for existing tests.
        let policies = vec![
            DeploymentPolicy::RequirePackages {
                packages: vec!["grafana".to_string()],
                strict: true,
            },
            DeploymentPolicy::RequireCveCheck {
                config: CveCheckConfig::default(),
            },
            DeploymentPolicy::RequirePackages {
                packages: vec!["neovim".to_string()],
                strict: true,
            },
        ];

        let policies_json = serde_json::json!({
            "hasRequiredPackages_0": true,
            "hasRequiredPackages_1": true,
        });

        let result =
            PolicyCheckResult::from_json("campground-host".to_string(), &policies_json, &policies);
        assert!(result.meets_requirements);
        assert!(result.failed_policies.is_empty());
    }

    #[test]
    fn cve_check_policy_excluded_from_nix_expression() {
        let id1 = uuid::Uuid::from_u128(1);
        let id2 = uuid::Uuid::from_u128(2);
        let mut map = PoliciesByConfiguration::new();
        map.insert(
            "test-config".to_string(),
            vec![
                AssignedPolicy {
                    policy_id: id1,
                    policy: DeploymentPolicy::RequireCrystalForgeAgent { strict: true },
                },
                AssignedPolicy {
                    policy_id: id2,
                    policy: DeploymentPolicy::RequireCveCheck {
                        config: CveCheckConfig::default(),
                    },
                },
            ],
        );
        let expr = build_nix_eval_expression("github:user/repo", &map);
        // CF-agent should appear; CVE check should not (not Nix-evaluated).
        assert!(expr.contains("crystal-forge-agent.enable"));
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
        let map = policies_map_for(vec![DeploymentPolicy::CustomCheck {
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
        }]);
        let expr = build_nix_eval_expression("github:user/repo", &map);
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

    // ── Regression tests for policy expression cfg scope ───────────────────

    #[test]
    fn bulk_package_checker_binds_full_cfg_object() {
        let mut map = PoliciesByConfiguration::new();
        map.insert(
            "gray".to_string(),
            vec![AssignedPolicy {
                policy_id: uuid::Uuid::from_u128(1),
                policy: DeploymentPolicy::RequirePackages {
                    packages: vec!["grafana".to_string()],
                    strict: true,
                },
            }],
        );

        let expr = build_nix_eval_expression("github:user/repo", &map);

        // Checker must bind the full nixosConfiguration object as `cfg`.
        assert!(
            expr.contains("\"gray\" = cfg:"),
            "bulk checker must bind full cfg object, got:\n{expr}"
        );
        // The package policy must reference the full cfg object.
        assert!(
            expr.contains("cfg.config.environment.systemPackages"),
            "package policy must use cfg.config scope, got:\n{expr}"
        );
        // The checker must be invoked with the full cfg object.
        assert!(
            expr.contains("checker cfg") && !expr.contains("checker cfg.config"),
            "checker must receive full cfg object, got:\n{expr}"
        );
        // Sanity: must not mix a `config` binding with a `cfg` expression.
        assert!(
            !expr.contains("\"gray\" = config:"),
            "checker must not bind `config` while expressions reference `cfg`, got:\n{expr}"
        );
    }

    #[test]
    fn standalone_agent_policy_uses_full_cfg_object() {
        let assigned = vec![AssignedPolicy {
            policy_id: uuid::Uuid::from_u128(1),
            policy: DeploymentPolicy::RequireCrystalForgeAgent { strict: true },
        }];

        let expr = crate::models::evaluate_with_policies::build_single_system_eval_expression(
            "github:user/repo",
            "gray",
            &assigned,
        );

        // Both the unconditional cfAgentEnabled and the assigned-policy stable
        // key must use cfg.config.
        assert!(
            expr.contains("cfg.config.systemd.services.crystal-forge-agent.enable"),
            "standalone agent check must use cfg.config scope, got:\n{expr}"
        );
        // Every occurrence of `config.systemd.services` must be prefixed with `cfg.`;
        // otherwise the expression references an unbound `config` variable.
        let cfg_count = expr.matches("cfg.config.systemd.services").count();
        let config_count = expr.matches("config.systemd.services").count();
        assert_eq!(
            cfg_count, config_count,
            "standalone expression must not reference unbound `config`, got:\n{expr}"
        );
    }

    #[test]
    fn bulk_custom_check_preserves_documented_cfg_scope() {
        let mut map = PoliciesByConfiguration::new();
        map.insert(
            "gray".to_string(),
            vec![AssignedPolicy {
                policy_id: uuid::Uuid::from_u128(1),
                policy: DeploymentPolicy::CustomCheck {
                    expression: "cfg.config.networking.firewall.enable".to_string(),
                    description: "firewall".to_string(),
                    field_name: "firewallEnabled".to_string(),
                    strict: true,
                    rules: Vec::new(),
                    mode: RuleMode::All,
                },
            }],
        );

        let expr = build_nix_eval_expression("github:user/repo", &map);

        assert!(
            expr.contains("\"gray\" = cfg:"),
            "bulk checker must bind full cfg object, got:\n{expr}"
        );
        assert!(
            expr.contains("cfg.config.networking.firewall.enable"),
            "custom check expression must be emitted verbatim with cfg scope, got:\n{expr}"
        );
    }

    #[test]
    fn bulk_multi_rule_custom_check_preserves_cfg_scope() {
        let mut map = PoliciesByConfiguration::new();
        map.insert(
            "gray".to_string(),
            vec![AssignedPolicy {
                policy_id: uuid::Uuid::from_u128(1),
                policy: DeploymentPolicy::CustomCheck {
                    expression: String::new(),
                    description: "ssh-and-firewall".to_string(),
                    field_name: "parent".to_string(),
                    strict: true,
                    rules: vec![
                        PolicyRule {
                            expression: "cfg.config.services.openssh.enable".to_string(),
                            description: "ssh".to_string(),
                            field_name: "sshEnabled".to_string(),
                            strict: true,
                        },
                        PolicyRule {
                            expression: "cfg.config.networking.firewall.enable".to_string(),
                            description: "firewall".to_string(),
                            field_name: "firewallEnabled".to_string(),
                            strict: true,
                        },
                    ],
                    mode: RuleMode::All,
                },
            }],
        );

        let expr = build_nix_eval_expression("github:user/repo", &map);

        assert!(
            expr.contains("\"gray\" = cfg:"),
            "bulk checker must bind full cfg object, got:\n{expr}"
        );
        assert!(
            expr.contains("cfg.config.services.openssh.enable"),
            "multi-rule expression must be emitted with cfg scope, got:\n{expr}"
        );
        assert!(
            expr.contains("cfg.config.networking.firewall.enable"),
            "multi-rule expression must be emitted with cfg scope, got:\n{expr}"
        );
    }

    /// Build a synthetic Nix expression that evaluates the policy fields
    /// produced for a single configuration without touching the network or a
    /// real flake, then run `nix eval --json` and assert the results.
    ///
    /// This is the critical regression test: it fails when the generated
    /// expression references an unbound variable (`cfg` vs `config`).
    #[test]
    #[ignore = "requires Nix evaluator in PATH"]
    fn generated_policy_fields_evaluate_without_undefined_variables() {
        let assigned = vec![
            AssignedPolicy {
                policy_id: uuid::Uuid::from_u128(1),
                policy: DeploymentPolicy::RequireCrystalForgeAgent { strict: true },
            },
            AssignedPolicy {
                policy_id: uuid::Uuid::from_u128(2),
                policy: DeploymentPolicy::RequirePackages {
                    packages: vec!["grafana".to_string()],
                    strict: true,
                },
            },
            AssignedPolicy {
                policy_id: uuid::Uuid::from_u128(3),
                policy: DeploymentPolicy::CustomCheck {
                    expression: String::new(),
                    description: "ssh-and-firewall".to_string(),
                    field_name: "parent".to_string(),
                    strict: true,
                    rules: vec![
                        PolicyRule {
                            expression: "cfg.config.services.openssh.enable".to_string(),
                            description: "ssh".to_string(),
                            field_name: "sshEnabled".to_string(),
                            strict: true,
                        },
                        PolicyRule {
                            expression: "cfg.config.networking.firewall.enable".to_string(),
                            description: "firewall".to_string(),
                            field_name: "firewallEnabled".to_string(),
                            strict: true,
                        },
                    ],
                    mode: RuleMode::All,
                },
            },
        ];

        let field_lines = build_policy_fields_for_config(&assigned);
        assert!(!field_lines.is_empty());

        let fields = field_lines.join("\n");
        let agent_key = policy_result_key(&uuid::Uuid::from_u128(1));
        let package_key = policy_result_key(&uuid::Uuid::from_u128(2));

        let expr = format!(
            r#"
let
  cfg = {{
    config = {{
      systemd.services.crystal-forge-agent.enable = true;
      services.crystal-forge.enable = true;
      services.crystal-forge.client.enable = true;
      services.openssh.enable = true;
      environment.systemPackages = [
        {{ pname = "grafana"; name = "grafana"; }}
      ];
      networking.firewall.enable = true;
    }};
  }};
in {{
  {fields}
}}
"#
        );

        let output = std::process::Command::new("nix")
            .args(["eval", "--json", "--expr", &expr])
            .output()
            .expect("failed to spawn nix eval");

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            panic!(
                "nix eval failed:\n{}\nGenerated expression:\n{}",
                stderr, expr
            );
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let parsed: serde_json::Value =
            serde_json::from_str(&stdout).expect("nix eval output must be valid JSON");

        assert_eq!(
            parsed.get(&agent_key).and_then(|v| v.as_bool()),
            Some(true),
            "agent policy must evaluate to true; got {parsed}"
        );
        assert_eq!(
            parsed.get(&package_key).and_then(|v| v.as_bool()),
            Some(true),
            "package policy must evaluate to true; got {parsed}"
        );
        assert_eq!(
            parsed.get("sshEnabled").and_then(|v| v.as_bool()),
            Some(true),
            "custom ssh check must evaluate to true; got {parsed}"
        );
        assert_eq!(
            parsed.get("firewallEnabled").and_then(|v| v.as_bool()),
            Some(true),
            "custom firewall check must evaluate to true; got {parsed}"
        );
    }

    #[test]
    fn cf_agent_metadata_non_boolean_is_parser_error() {
        let policies_json = serde_json::json!({
            "cfAgentEnabled": "true",
        });

        let result = PolicyCheckResult::from_assigned("gray".to_string(), &policies_json, &[]);

        assert!(
            result.is_err(),
            "non-boolean cfAgentEnabled must be treated as an infrastructure error"
        );
        let err = result.unwrap_err();
        assert!(
            err.contains("must be boolean"),
            "error should mention boolean requirement: {err}"
        );
    }

    #[test]
    fn assigned_boolean_policy_non_boolean_is_parser_error() {
        let policy_id = uuid::Uuid::from_u128(1);
        let assigned = vec![AssignedPolicy {
            policy_id,
            policy: DeploymentPolicy::RequirePackages {
                packages: vec!["grafana".to_string()],
                strict: true,
            },
        }];
        let key = policy_result_key(&policy_id);
        let policies_json = serde_json::json!({
            "cfAgentEnabled": true,
            key: "true",
        });

        let result =
            PolicyCheckResult::from_assigned("gray".to_string(), &policies_json, &assigned);

        assert!(
            result.is_err(),
            "non-boolean assigned policy value must be treated as an infrastructure error"
        );
        let err = result.unwrap_err();
        assert!(
            err.contains("must be boolean"),
            "error should mention boolean requirement: {err}"
        );
    }

    #[test]
    fn custom_check_non_boolean_value_is_parser_error() {
        let policy_id = uuid::Uuid::from_u128(1);
        let assigned = vec![AssignedPolicy {
            policy_id,
            policy: DeploymentPolicy::CustomCheck {
                expression: "cfg.config.networking.firewall.enable".to_string(),
                description: "firewall".to_string(),
                field_name: "firewallEnabled".to_string(),
                strict: true,
                rules: Vec::new(),
                mode: RuleMode::All,
            },
        }];
        let policies_json = serde_json::json!({
            "cfAgentEnabled": true,
            "firewallEnabled": "true",
        });

        let result =
            PolicyCheckResult::from_assigned("gray".to_string(), &policies_json, &assigned);

        assert!(
            result.is_err(),
            "non-boolean custom check value must be treated as an infrastructure error"
        );
        let err = result.unwrap_err();
        assert!(
            err.contains("must evaluate to boolean"),
            "error should mention boolean requirement: {err}"
        );
    }

    // ── Reserved result-field tests ────────────────────────────────────────

    #[test]
    fn reserved_policy_result_fields_include_cf_agent_enabled() {
        assert!(is_reserved_policy_result_field("cfAgentEnabled"));
        assert!(is_reserved_policy_result_field("cfagentenabled"));
        assert!(!is_reserved_policy_result_field("firewallEnabled"));
    }

    #[test]
    fn build_policy_fields_skips_legacy_custom_check_with_reserved_field_name() {
        let assigned = vec![AssignedPolicy {
            policy_id: uuid::Uuid::from_u128(1),
            policy: DeploymentPolicy::CustomCheck {
                expression: "true".to_string(),
                description: "spoof agent".to_string(),
                field_name: "cfAgentEnabled".to_string(),
                strict: true,
                rules: Vec::new(),
                mode: RuleMode::All,
            },
        }];

        let fields = build_policy_fields_for_config(&assigned);

        assert!(
            !fields
                .iter()
                .any(|line| line.contains("cfAgentEnabled = true")),
            "custom check must not be allowed to emit reserved result field, got:\n{fields:#?}"
        );
    }

    #[test]
    fn build_policy_fields_skips_multi_rule_with_reserved_field_name() {
        let assigned = vec![AssignedPolicy {
            policy_id: uuid::Uuid::from_u128(1),
            policy: DeploymentPolicy::CustomCheck {
                expression: String::new(),
                description: "mixed rules".to_string(),
                field_name: "parent".to_string(),
                strict: true,
                rules: vec![
                    PolicyRule {
                        expression: "true".to_string(),
                        description: "spoof agent".to_string(),
                        field_name: "cfAgentEnabled".to_string(),
                        strict: true,
                    },
                    PolicyRule {
                        expression: "true".to_string(),
                        description: "legit".to_string(),
                        field_name: "firewallEnabled".to_string(),
                        strict: true,
                    },
                ],
                mode: RuleMode::All,
            },
        }];

        let fields = build_policy_fields_for_config(&assigned);

        assert!(
            !fields
                .iter()
                .any(|line| line.contains("cfAgentEnabled = true")),
            "custom-check rule must not be allowed to emit reserved result field, got:\n{fields:#?}"
        );
        assert!(
            fields
                .iter()
                .any(|line| line.contains("\"firewallEnabled\" = true")),
            "non-reserved rule should still be emitted, got:\n{fields:#?}"
        );
    }

    #[test]
    fn strict_package_failure_persists_requirements_false_and_policy_json() {
        let policy_id = uuid::Uuid::from_u128(1);
        let assigned = vec![AssignedPolicy {
            policy_id,
            policy: DeploymentPolicy::RequirePackages {
                packages: vec!["grafana".to_string()],
                strict: true,
            },
        }];
        let check = PolicyCheckResult::from_assigned(
            "gray".to_string(),
            &serde_json::json!({
                "cfAgentEnabled": true,
                policy_result_key(&policy_id): false,
            }),
            &assigned,
        )
        .expect("policy metadata should parse");

        assert!(!policy_requirements_met(&check));

        let persisted = policy_results_json(&check, &assigned);
        let result = persisted
            .get("assigned")
            .and_then(|assigned| assigned.get(policy_id.to_string()))
            .expect("assigned policy result should be persisted");
        assert_eq!(
            result.get("type").and_then(|value| value.as_str()),
            Some("require_packages")
        );
        assert_eq!(
            result.get("passed").and_then(|value| value.as_bool()),
            Some(false)
        );
        assert_eq!(
            result.get("details").and_then(|value| value.as_str()),
            Some("Missing required packages: grafana")
        );
    }
}
