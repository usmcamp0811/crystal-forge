//! Defines deployment-policy configuration, validation, and evaluator
//! metadata.
//!
//! Composite policy versions are immutable typed rule sets. Validation
//! protects persistence boundaries. Evaluation preserves each rule's phase and
//! outcome so an error or missing observation cannot become a pass.

use serde::{Deserialize, Serialize};
use sqlx::types::chrono::{DateTime, Utc};
use std::collections::{BTreeMap, HashMap};
use std::fmt::Write;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use tracing::warn;
use uuid::Uuid;

/// Identifies the typed heterogeneous deployment-policy format.
pub const COMPOSITE_POLICY_TYPE: &str = "composite";

/// Maximum number of rules accepted in one composite policy version.
///
/// This limit bounds validation, generated Nix expressions, persisted outcome
/// rows, and authorization work for each immutable policy version.
pub const MAX_COMPOSITE_RULES: usize = 64;

/// Defines one immutable composite policy version.
///
/// Valid configurations use schema version 1, `all` mode, unique non-nil rule
/// IDs, and between 1 and [`MAX_COMPOSITE_RULES`] rules.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompositePolicyConfig {
    /// Selects the serialized composite schema; the only valid value is 1.
    pub schema_version: u8,
    /// Selects how constituent outcomes form the aggregate outcome.
    pub mode: CompositeRuleMode,
    /// Preserves the ordered immutable rules in this policy version.
    pub rules: Vec<CompositeRule>,
}

/// Selects the aggregate semantics for a composite policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompositeRuleMode {
    /// Requires every constituent rule to pass.
    All,
}

/// Associates a stable rule identity with one typed rule configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompositeRule {
    /// Identifies the rule within every assessment of this policy version.
    pub id: Uuid,
    /// Defines the rule's evidence source and comparison semantics.
    #[serde(flatten)]
    pub rule: CompositeRuleKind,
}

/// Defines the supported evidence and enforcement rule kinds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "config", rename_all = "snake_case")]
pub enum CompositeRuleKind {
    /// Compares one evaluated NixOS option with a typed expected value.
    NixosOption(NixosOptionRuleConfig),
    /// Requires each named package in direct `environment.systemPackages` entries.
    PackagesInstalled(PackagesInstalledRuleConfig),
    /// Prohibits each named package in direct `environment.systemPackages` entries.
    PackagesAbsent(PackagesAbsentRuleConfig),
    /// Evaluates a bounded custom Nix Boolean expression.
    CustomEval(CustomEvalRuleConfig),
    /// Limits CVEs at or above a configured severity.
    CveBlock(CveBlockRuleConfig),
    /// Requires the configuration evaluation to finish successfully.
    EvalPassed(EmptyRuleConfig),
    /// Requires Nix to resolve the exact requested immutable revision.
    PinRequired(EmptyRuleConfig),
    /// Restricts deployment to a validated local-time window.
    TimeWindow(TimeWindowRuleConfig),
}

impl CompositeRuleKind {
    /// Returns the stable snake-case kind used in persistence and evidence.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::NixosOption(_) => "nixos_option",
            Self::PackagesInstalled(_) => "packages_installed",
            Self::PackagesAbsent(_) => "packages_absent",
            Self::CustomEval(_) => "custom_eval",
            Self::CveBlock(_) => "cve_block",
            Self::EvalPassed(_) => "eval_passed",
            Self::PinRequired(_) => "pin_required",
            Self::TimeWindow(_) => "time_window",
        }
    }
}

/// Classifies the expected JSON and Nix value for an option comparison.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NixosOptionValueType {
    /// Requires a Boolean value.
    Boolean,
    /// Requires a string selected from an option enum.
    Enum,
    /// Requires a signed integer value.
    Integer,
    /// Requires a string value.
    String,
    /// Requires a string whose line boundaries are significant.
    Lines,
    /// Requires a string when metadata cannot infer a safer type.
    Unknown,
}

/// Configures a typed comparison against one NixOS option.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NixosOptionRuleConfig {
    /// Gives a dotted Nix attribute path with optional JSON-quoted segments.
    pub path: String,
    /// Gives the comparison operator allowed by `value_type`.
    pub operator: String,
    /// Determines the accepted operators and JSON value shape.
    pub value_type: NixosOptionValueType,
    /// Gives the expected value and must match `value_type`.
    pub value: serde_json::Value,
}

/// Configures package identities that must be directly installed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackagesInstalledRuleConfig {
    /// Lists unique package `pname` values to require.
    pub packages: Vec<String>,
}

/// Configures package identities that must be absent from direct installation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackagesAbsentRuleConfig {
    /// Lists package `pname` values to prohibit.
    pub packages: Vec<String>,
}

/// Represents a rule kind that has no additional configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct EmptyRuleConfig {}

/// Configures one contained custom Nix evaluation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CustomEvalRuleConfig {
    /// Gives a Nix expression evaluated with `config` in scope.
    pub expression: String,
    /// Gives operator-facing detail when the expression fails.
    pub message: String,
}

/// Selects the minimum CVE severity counted by a CVE block rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CveBlockSeverity {
    /// Counts critical CVEs.
    Critical,
    /// Counts high and critical CVEs.
    High,
    /// Counts medium, high, and critical CVEs.
    Medium,
    /// Counts all known severity levels.
    Low,
}

/// Configures a maximum CVE count at or above one severity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CveBlockRuleConfig {
    /// Selects the minimum counted severity.
    pub severity: CveBlockSeverity,
    /// Gives the largest count that still passes.
    pub max_allowed: u32,
}

/// Configures an allowed deployment window in one IANA timezone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TimeWindowRuleConfig {
    /// Lists normalized weekdays on which the window applies.
    pub days: Vec<String>,
    /// Gives the inclusive local start time in `HH:MM` form.
    pub from: String,
    /// Gives the local end time in `HH:MM` form.
    pub to: String,
    /// Gives the IANA timezone used to interpret local times.
    pub tz: String,
}

/// Identifies the lifecycle phase that produced a rule outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnforcementPhase {
    /// Indicates configuration evaluation evidence.
    Evaluation,
    /// Indicates vulnerability-scan evidence.
    Scan,
    /// Indicates deployment-time evidence.
    Deployment,
}

/// Represents a normalized constituent rule result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnforcementOutcome {
    /// Indicates authoritative evidence satisfied the rule.
    Pass,
    /// Indicates authoritative evidence violated the rule.
    Fail,
    /// Indicates the rule could not evaluate its available evidence.
    Error,
    /// Indicates that the required phase or evidence has not completed.
    NotChecked,
}

/// Preserves one rule's normalized result and source-specific evidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompositeRuleOutcome {
    /// Identifies the immutable rule within its policy version.
    pub rule_id: Uuid,
    /// Gives the stable serialized rule kind.
    pub kind: String,
    /// Identifies the phase that produced this outcome.
    pub phase: EnforcementPhase,
    /// Gives the normalized outcome without collapsing errors into failures.
    pub outcome: EnforcementOutcome,
    /// Indicates whether this outcome prevents enforcement from proceeding.
    pub blocking: bool,
    /// Explains the outcome for operators and audit records.
    pub detail: String,
    /// Preserves phase-specific facts used to derive the outcome.
    pub evidence: serde_json::Value,
}

/// Returns the conservative aggregate of constituent composite outcomes.
///
/// Precedence is error, fail, not checked, then pass. An empty input is not
/// checked, so absent evidence never produces a pass.
pub fn aggregate_composite_outcomes(outcomes: &[CompositeRuleOutcome]) -> EnforcementOutcome {
    if outcomes
        .iter()
        .any(|result| result.outcome == EnforcementOutcome::Error)
    {
        EnforcementOutcome::Error
    } else if outcomes
        .iter()
        .any(|result| result.outcome == EnforcementOutcome::Fail)
    {
        EnforcementOutcome::Fail
    } else if outcomes
        .iter()
        .any(|result| result.outcome == EnforcementOutcome::NotChecked)
    {
        EnforcementOutcome::NotChecked
    } else if !outcomes.is_empty()
        && outcomes
            .iter()
            .all(|result| result.outcome == EnforcementOutcome::Pass)
    {
        EnforcementOutcome::Pass
    } else {
        EnforcementOutcome::NotChecked
    }
}

/// Returns the canonical semantic digest for a composite configuration.
pub fn composite_config_digest(config: &CompositePolicyConfig) -> String {
    let value = serde_json::to_value(config).unwrap_or_else(|_| serde_json::Value::Null);
    crate::compliance::canonical::semantic_digest(&value)
}

const MAX_CUSTOM_EVAL_EXPRESSION_BYTES: usize = 16 * 1024;

fn parse_nixos_option_path(path: &str) -> Result<Vec<String>, String> {
    let path = path.trim();
    if path.is_empty() {
        return Err("path must not be empty".to_string());
    }

    let bytes = path.as_bytes();
    let mut offset = 0;
    let mut segments = Vec::new();
    while offset < bytes.len() {
        if bytes[offset] == b'"' {
            let start = offset;
            offset += 1;
            let mut escaped = false;
            while offset < bytes.len() {
                let byte = bytes[offset];
                offset += 1;
                if escaped {
                    escaped = false;
                } else if byte == b'\\' {
                    escaped = true;
                } else if byte == b'"' {
                    break;
                }
            }
            if offset > bytes.len() || bytes.get(offset - 1) != Some(&b'"') {
                return Err("quoted segment is not terminated".to_string());
            }
            let segment: String = serde_json::from_str(&path[start..offset])
                .map_err(|error| format!("invalid quoted segment: {error}"))?;
            if segment.is_empty() {
                return Err("segments must not be empty".to_string());
            }
            segments.push(segment);
        } else {
            let start = offset;
            while offset < bytes.len() && bytes[offset] != b'.' {
                let byte = bytes[offset];
                if byte == b'"'
                    || byte == b'\\'
                    || byte.is_ascii_whitespace()
                    || byte.is_ascii_control()
                {
                    return Err("bare segments cannot contain quotes, escapes, whitespace, or control characters".to_string());
                }
                offset += 1;
            }
            if start == offset {
                return Err("segments must not be empty".to_string());
            }
            segments.push(path[start..offset].to_string());
        }

        if offset == bytes.len() {
            break;
        }
        if bytes[offset] != b'.' {
            return Err("quoted segments must be separated by dots".to_string());
        }
        offset += 1;
        if offset == bytes.len() {
            return Err("segments must not be empty".to_string());
        }
    }
    Ok(segments)
}

fn validate_package_pname(pname: &str) -> Result<(), String> {
    if pname.is_empty() || pname.len() > 255 {
        return Err("must be between 1 and 255 bytes".to_string());
    }
    if !pname
        .bytes()
        .next()
        .is_some_and(|byte| byte.is_ascii_alphanumeric())
        || !pname
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'-' | b'.' | b'_'))
    {
        return Err(
            "must be a package pname (ASCII letters, digits, '+', '-', '.', or '_')".to_string(),
        );
    }
    Ok(())
}

fn validate_custom_eval_syntax(expressions: &[&str]) -> Result<(), String> {
    // Parse every custom expression in one bounded subprocess. This keeps the
    // synchronous persistence API while avoiding one blocking process and
    // timeout for every rule on an async request path.
    let wrapped = format!(
        "config: [ {} ]",
        expressions
            .iter()
            .map(|expression| format!("({expression})"))
            .collect::<Vec<_>>()
            .join(" ")
    );
    let mut child = Command::new("nix-instantiate")
        // Parsing needs no store. The dummy store also avoids initializing
        // local Nix state in restricted package-build and service sandboxes.
        .args(["--store", "dummy://", "--parse", "--expr", &wrapped])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("could not run the Nix parser: {error}"))?;
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        if child
            .try_wait()
            .map_err(|error| format!("could not wait for the Nix parser: {error}"))?
            .is_some()
        {
            break;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err("Nix syntax validation timed out after 2 seconds".to_string());
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let output = child
        .wait_with_output()
        .map_err(|error| format!("could not collect Nix parser output: {error}"))?;
    if output.status.success() {
        return Ok(());
    }
    let diagnostic = String::from_utf8_lossy(&output.stderr);
    let diagnostic = diagnostic.trim().chars().take(500).collect::<String>();
    Err(format!("invalid Nix expression syntax: {diagnostic}"))
}

/// Validates the exact policy type and config at each persistence boundary.
/// Non-composite policy validation remains on its legacy paths.
fn decode_policy_type_config(
    policy_type: &str,
    config: &serde_json::Value,
    validate_nix_syntax: bool,
) -> Result<Option<CompositePolicyConfig>, String> {
    if policy_type != COMPOSITE_POLICY_TYPE {
        return Ok(None);
    }

    let composite: CompositePolicyConfig = serde_json::from_value(config.clone())
        .map_err(|error| format!("invalid composite policy config: {error}"))?;
    if composite.schema_version != 1 {
        return Err("composite config.schema_version must be 1".to_string());
    }
    if composite.rules.is_empty() {
        return Err("composite config.rules must not be empty".to_string());
    }
    if composite.rules.len() > MAX_COMPOSITE_RULES {
        return Err(format!(
            "composite config.rules must contain at most {MAX_COMPOSITE_RULES} rules"
        ));
    }

    let mut ids = std::collections::HashSet::with_capacity(composite.rules.len());
    let mut custom_eval_expressions = Vec::new();
    for (index, rule) in composite.rules.iter().enumerate() {
        if rule.id.is_nil() {
            return Err(format!(
                "composite config.rules[{index}].id must not be nil"
            ));
        }
        if !ids.insert(rule.id) {
            return Err(format!(
                "composite config.rules[{index}].id {} is duplicated",
                rule.id
            ));
        }

        match &rule.rule {
            CompositeRuleKind::NixosOption(option) => {
                parse_nixos_option_path(&option.path).map_err(|error| {
                    format!("composite config.rules[{index}].config.path is invalid: {error}")
                })?;
                let valid_operator = match option.value_type {
                    NixosOptionValueType::Boolean
                    | NixosOptionValueType::Enum
                    | NixosOptionValueType::String
                    | NixosOptionValueType::Lines
                    | NixosOptionValueType::Unknown => {
                        matches!(option.operator.as_str(), "==" | "!=")
                    }
                    NixosOptionValueType::Integer => {
                        matches!(option.operator.as_str(), "==" | "!=" | ">=" | "<=")
                    }
                };
                if !valid_operator {
                    return Err(format!(
                        "composite config.rules[{index}].config.operator is invalid for {:?}",
                        option.value_type
                    ));
                }
                let valid_value = match option.value_type {
                    NixosOptionValueType::Boolean => option.value.is_boolean(),
                    NixosOptionValueType::Integer => option.value.as_i64().is_some(),
                    NixosOptionValueType::Enum
                    | NixosOptionValueType::String
                    | NixosOptionValueType::Lines
                    | NixosOptionValueType::Unknown => option.value.is_string(),
                };
                if !valid_value {
                    return Err(format!(
                        "composite config.rules[{index}].config.value does not match value_type"
                    ));
                }
            }
            CompositeRuleKind::PackagesInstalled(packages) => {
                if packages.packages.is_empty()
                    || packages
                        .packages
                        .iter()
                        .any(|package| package.trim().is_empty())
                {
                    return Err(format!(
                        "composite config.rules[{index}].config.packages must contain non-empty strings"
                    ));
                }
                for package in &packages.packages {
                    validate_package_pname(package).map_err(|error| {
                        format!(
                            "composite config.rules[{index}].config.packages contains invalid pname {package:?}: {error}"
                        )
                    })?;
                }
            }
            CompositeRuleKind::PackagesAbsent(packages) => {
                if packages.packages.is_empty()
                    || packages
                        .packages
                        .iter()
                        .any(|package| package.trim().is_empty())
                {
                    return Err(format!(
                        "composite config.rules[{index}].config.packages must contain non-empty strings"
                    ));
                }
                for package in &packages.packages {
                    validate_package_pname(package).map_err(|error| {
                        format!(
                            "composite config.rules[{index}].config.packages contains invalid pname {package:?}: {error}"
                        )
                    })?;
                }
            }
            CompositeRuleKind::CustomEval(custom) => {
                if custom.expression.trim().is_empty() {
                    return Err(format!(
                        "composite config.rules[{index}].config.expression must not be empty"
                    ));
                }
                if custom.expression.len() > MAX_CUSTOM_EVAL_EXPRESSION_BYTES {
                    return Err(format!(
                        "composite config.rules[{index}].config.expression exceeds the {} byte limit",
                        MAX_CUSTOM_EVAL_EXPRESSION_BYTES
                    ));
                }
                if validate_nix_syntax {
                    custom_eval_expressions.push(custom.expression.as_str());
                }
            }
            CompositeRuleKind::CveBlock(_) => {}
            CompositeRuleKind::EvalPassed(_) | CompositeRuleKind::PinRequired(_) => {}
            CompositeRuleKind::TimeWindow(window) => {
                crate::services::time_window_policy::validate_window_parts(
                    &window.days,
                    &window.from,
                    &window.to,
                    &window.tz,
                )
                .map_err(|error| format!("composite config.rules[{index}].config: {error}"))?;
            }
        }
    }
    if !custom_eval_expressions.is_empty() {
        validate_custom_eval_syntax(&custom_eval_expressions)
            .map_err(|error| format!("composite custom_eval expression is invalid: {error}"))?;
    }

    Ok(Some(composite))
}

/// Decodes and structurally validates persisted policy data without a parser.
///
/// Runtime authorization, scanning, and evaluation paths must use this
/// function. Immutable write and import boundaries use the full validator.
///
/// # Errors
///
/// Returns an error when a composite configuration violates its schema, rule
/// identity, type, operator, value, package-name, or time-window constraints.
pub fn deserialize_policy_type_config(
    policy_type: &str,
    config: &serde_json::Value,
) -> Result<Option<CompositePolicyConfig>, String> {
    decode_policy_type_config(policy_type, config, false)
}

/// Fully validates policy input at immutable persistence boundaries.
///
/// Custom expressions are parsed in one bounded Nix subprocess in addition to
/// the structural checks performed by [`deserialize_policy_type_config`].
///
/// # Errors
///
/// Returns a structural validation error, a Nix syntax error, an error starting
/// or collecting the parser, or a parser timeout.
pub fn validate_policy_type_config(
    policy_type: &str,
    config: &serde_json::Value,
) -> Result<Option<CompositePolicyConfig>, String> {
    decode_policy_type_config(policy_type, config, true)
}

/// Validates policy input without blocking the async executor.
///
/// This function preserves the contract of [`validate_policy_type_config`] but
/// runs its bounded synchronous Nix parser invocation on Tokio's blocking pool.
/// Async request, query, and import paths must use this function. Offline tools
/// and unit tests may use [`validate_policy_type_config`] directly.
///
/// # Errors
///
/// Returns the same validation errors as [`validate_policy_type_config`]. Also
/// returns an error if Tokio cannot join the blocking validation task, including
/// when the task panics.
pub async fn validate_policy_type_config_async(
    policy_type: &str,
    config: &serde_json::Value,
) -> Result<Option<CompositePolicyConfig>, String> {
    let policy_type = policy_type.to_owned();
    let config = config.clone();
    tokio::task::spawn_blocking(move || validate_policy_type_config(&policy_type, &config))
        .await
        .map_err(|error| format!("policy validation task failed: {error}"))?
}

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
    /// Typed heterogeneous policy executed by the phase-specific composite
    /// evaluator. Legacy single-expression helpers cannot represent this variant.
    Composite {
        /// Defines the immutable typed rules evaluated by their owning phases.
        config: CompositePolicyConfig,
    },
}

impl DeploymentPolicy {
    /// Returns whether a failed policy blocks deployment.
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
            DeploymentPolicy::Composite { .. } => true,
        }
    }

    /// Returns an operator-facing summary of the configured policy behavior.
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
            DeploymentPolicy::Composite { config } => {
                format!("Composite policy ({} rules, all mode)", config.rules.len())
            }
        }
    }

    /// Returns true if this policy is evaluated via Nix (nix-eval-jobs path).
    /// RequireCveCheck and new policy types are DB/deployment-time evaluated.
    pub fn is_nix_evaluated(&self) -> bool {
        match self {
            DeploymentPolicy::RequireCveCheck { .. }
            | DeploymentPolicy::TimeWindow { .. }
            | DeploymentPolicy::RequireApprovals { .. }
            | DeploymentPolicy::CanaryRollout { .. }
            | DeploymentPolicy::CveThreshold { .. } => false,
            DeploymentPolicy::Composite { config } => config.rules.iter().any(|rule| {
                !matches!(
                    &rule.rule,
                    CompositeRuleKind::CveBlock(_) | CompositeRuleKind::TimeWindow(_)
                )
            }),
            _ => true,
        }
    }

    /// Returns the legacy Nix expression fragment for one single-expression policy.
    ///
    /// Composite policies use stable per-rule fields generated by
    /// [`build_policy_fields_for_config_standalone`] and cannot be flattened to
    /// one field without losing constituent outcomes.
    ///
    /// # Errors
    ///
    /// Returns an error for policies that are not represented by one Nix
    /// expression, including composite and deployment-phase policies.
    pub fn to_nix_expression_with_index(&self, index: usize) -> Result<(String, String), String> {
        let expression = match self {
            DeploymentPolicy::RequireCrystalForgeAgent { .. } => (
                "cfAgentEnabled".to_string(),
                "(config.systemd.services.crystal-forge-agent.enable or false) || \
                 ((config.services.crystal-forge.enable or false) && \
                  (config.services.crystal-forge.client.enable or false))"
                    .to_string(),
            ),
            DeploymentPolicy::RequirePackages { packages, .. } => {
                if packages.is_empty() {
                    return Ok((format!("hasRequiredPackages_{index}"), "false".to_string()));
                }
                let package_list = packages
                    .iter()
                    .map(|p| nix_string(p))
                    .collect::<Vec<_>>()
                    .join(" ");
                (
                    format!("hasRequiredPackages_{index}"),
                    format!(
                        // This contract intentionally checks direct systemPackages
                        // entries by pname; it does not claim closure membership.
                        "builtins.all \
                          (required: builtins.any \
                            (pkg: (pkg.pname or \"\") == required) \
                            config.environment.systemPackages) \
                         [ {} ]",
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
            DeploymentPolicy::RequireCveCheck { .. }
            | DeploymentPolicy::TimeWindow { .. }
            | DeploymentPolicy::RequireApprovals { .. }
            | DeploymentPolicy::CanaryRollout { .. }
            | DeploymentPolicy::CveThreshold { .. } => {
                return Err("policy is not evaluated by Nix".to_string());
            }
            DeploymentPolicy::Composite { .. } => {
                return Err(
                    "composite policies require phase-specific per-rule evaluation".to_string(),
                );
            }
        };
        Ok(expression)
    }

    /// Returns the legacy Nix expression fragment at index zero.
    ///
    /// # Errors
    ///
    /// Returns an error when [`Self::to_nix_expression_with_index`] cannot
    /// represent the policy as one Nix expression.
    pub fn to_nix_expression(&self) -> Result<(String, String), String> {
        self.to_nix_expression_with_index(0)
    }

    /// Returns the legacy JSON field name for one single-expression policy.
    ///
    /// # Errors
    ///
    /// Returns an error for composite policies because each evaluation-phase
    /// rule has its own stable policy-version/rule field key.
    pub fn field_name_with_index(&self, index: usize) -> Result<String, String> {
        let field_name = match self {
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
            DeploymentPolicy::Composite { .. } => {
                return Err("composite policies use stable per-rule field keys".to_string());
            }
        };
        Ok(field_name)
    }

    /// Returns the legacy JSON field name at index zero.
    ///
    /// # Errors
    ///
    /// Returns an error when [`Self::field_name_with_index`] cannot represent
    /// the policy with one field name.
    pub fn field_name(&self) -> Result<String, String> {
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
    /// The policy's real database name (`deployment_policies.name`), as
    /// distinct from `policy.description()` (a human-readable summary of
    /// the policy's configured behavior). The matrix and "View policy
    /// definition" navigation must use this field — the Policies page
    /// looks up definitions by their DB name, not by a generated
    /// description string.
    pub policy_name: String,
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
pub const RESERVED_POLICY_RESULT_FIELDS: &[&str] = &[
    "cfAgentEnabled",
    "requestedSourceRevision",
    "resolvedSourceRevision",
];

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
        DeploymentPolicy::Composite { .. } => COMPOSITE_POLICY_TYPE,
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

/// Fallback pass/fail resolution used only when a `PolicyCheckResult` has no
/// entry in `assigned_results` for this policy (the legacy `from_json` path,
/// or a hand-constructed synthetic result). This is intentionally NOT used as
/// the primary source: `has_required_packages` is a single shared field, so
/// when two `RequirePackages` policies are assigned to the same
/// configuration this fallback cannot distinguish between them.
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

/// Resolve whether an assigned policy passed, preferring the per-UUID
/// `assigned_results` map (correct for multiple policies of the same type)
/// and falling back to the shared compatibility fields only when no
/// per-UUID entry exists.
fn resolve_assigned_policy_passed(
    assigned_policy: &AssignedPolicy,
    check: &PolicyCheckResult,
) -> Option<bool> {
    if let Some(result) = check.assigned_results.get(&assigned_policy.policy_id) {
        return result.passed;
    }
    assigned_policy_passed(&assigned_policy.policy, check)
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

    for assigned_policy in assigned.iter().filter(|ap| {
        ap.policy.is_nix_evaluated() || matches!(ap.policy, DeploymentPolicy::Composite { .. })
    }) {
        let passed = resolve_assigned_policy_passed(assigned_policy, check);
        let mut persisted = serde_json::json!({
            // The real database name (`deployment_policies.name`) is used by
            // matrix navigation and must remain distinct from description.
            "name": assigned_policy.policy_name,
            "description": assigned_policy.policy.description(),
            "type": policy_kind(&assigned_policy.policy),
            "strict": assigned_policy.policy.is_strict(),
            "passed": passed,
            "details": policy_result_detail(&assigned_policy.policy, passed),
        });
        if let DeploymentPolicy::Composite { config } = &assigned_policy.policy {
            if let Some(object) = persisted.as_object_mut() {
                object.insert(
                    "config_digest".to_string(),
                    serde_json::Value::String(composite_config_digest(config)),
                );
                object.insert(
                    "rule_outcomes".to_string(),
                    serde_json::to_value(
                        check
                            .assigned_results
                            .get(&assigned_policy.policy_id)
                            .map(|result| result.composite_outcomes.as_slice())
                            .unwrap_or(&[]),
                    )
                    .unwrap_or_else(|_| serde_json::Value::Array(Vec::new())),
                );
            }
        }
        assigned_results.insert(assigned_policy.policy_id.to_string(), persisted);
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

/// Per-policy outcome, keyed by the policy's stable database UUID.
///
/// This is the source of truth for the persisted `policy_results` JSON and
/// the policy matrix. Unlike `has_required_packages` (a single shared field),
/// this map holds one entry per assigned policy, so two `RequirePackages`
/// policies on the same configuration do not collapse to a single boolean.
#[derive(Debug, Clone)]
pub struct AssignedPolicyCheckResult {
    pub passed: Option<bool>,
    /// Preserves each constituent result for composite policies.
    pub composite_outcomes: Vec<CompositeRuleOutcome>,
}

/// Results from checking deployment policies for a single system
#[derive(Debug, Clone)]
pub struct PolicyCheckResult {
    pub system_name: String,
    pub cf_agent_enabled: Option<bool>,
    /// Per-policy outcomes keyed by stable UUID. Populated by `from_assigned`;
    /// empty for the legacy `from_json` path (which has no stable UUIDs) and
    /// for synthetic/manually-constructed results.
    pub assigned_results: BTreeMap<Uuid, AssignedPolicyCheckResult>,
    /// Compatibility field: the *last* `RequirePackages` policy's result.
    /// Do not use this to reconstruct per-policy UI state — use
    /// `assigned_results` instead. Retained for the build-gate predicate and
    /// existing callers that only ever assign a single package policy.
    pub has_required_packages: Option<bool>,
    pub custom_checks: HashMap<String, bool>,
    pub meets_requirements: bool,
    pub warnings: Vec<String>,
    /// Tracks which policies failed (description, is_strict)
    pub failed_policies: Vec<(String, bool)>,
    /// CVE gate outcomes (populated after DB evaluation)
    pub cve_checks: Vec<CveCheckOutcome>,
}

/// Classifies a configuration evaluation that did not produce normal metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvaluationTerminalOutcome {
    /// Indicates that evaluation completed with a confirmed policy failure.
    ConfirmedFailure,
    /// Indicates that infrastructure or evaluation execution failed.
    Error,
    /// Indicates that evaluation has not reached a terminal outcome.
    Pending,
}

impl EvaluationTerminalOutcome {
    fn enforcement_outcome(self) -> EnforcementOutcome {
        match self {
            Self::ConfirmedFailure => EnforcementOutcome::Fail,
            Self::Error => EnforcementOutcome::Error,
            Self::Pending => EnforcementOutcome::NotChecked,
        }
    }
}

impl PolicyCheckResult {
    /// Creates composite rule evidence for an abnormal evaluation state.
    ///
    /// Evaluation-dependent rules become error or not checked as appropriate;
    /// scan and deployment rules remain absent until their owning phase runs.
    pub fn for_evaluation_terminal(
        system_name: String,
        assigned: &[AssignedPolicy],
        terminal: EvaluationTerminalOutcome,
        detail: &str,
    ) -> Self {
        let terminal_outcome = terminal.enforcement_outcome();
        let mut assigned_results = BTreeMap::new();
        let mut failed_policies = Vec::new();

        for assigned_policy in assigned {
            let DeploymentPolicy::Composite { config } = &assigned_policy.policy else {
                continue;
            };
            let outcomes = config
                .rules
                .iter()
                .filter_map(|rule| {
                    let outcome = if matches!(rule.rule, CompositeRuleKind::EvalPassed(_)) {
                        terminal_outcome
                    } else if matches!(
                        rule.rule,
                        CompositeRuleKind::NixosOption(_)
                            | CompositeRuleKind::PackagesInstalled(_)
                            | CompositeRuleKind::PackagesAbsent(_)
                            | CompositeRuleKind::CustomEval(_)
                            | CompositeRuleKind::PinRequired(_)
                    ) {
                        match terminal {
                            EvaluationTerminalOutcome::Error => EnforcementOutcome::Error,
                            _ => EnforcementOutcome::NotChecked,
                        }
                    } else {
                        return None;
                    };
                    Some(CompositeRuleOutcome {
                        rule_id: rule.id,
                        kind: rule.rule.kind().to_string(),
                        phase: EnforcementPhase::Evaluation,
                        outcome,
                        blocking: outcome != EnforcementOutcome::Pass,
                        detail: detail.to_string(),
                        evidence: serde_json::json!({
                            "configuration": system_name,
                            "terminal_outcome": match terminal {
                                EvaluationTerminalOutcome::ConfirmedFailure => "confirmed_failure",
                                EvaluationTerminalOutcome::Error => "error",
                                EvaluationTerminalOutcome::Pending => "pending",
                            },
                        }),
                    })
                })
                .collect::<Vec<_>>();
            let passed = match terminal {
                EvaluationTerminalOutcome::Pending => None,
                _ => Some(false),
            };
            if passed == Some(false) {
                failed_policies.push((assigned_policy.policy.description(), true));
            }
            assigned_results.insert(
                assigned_policy.policy_id,
                AssignedPolicyCheckResult {
                    passed,
                    composite_outcomes: outcomes,
                },
            );
        }

        Self {
            system_name,
            cf_agent_enabled: None,
            assigned_results,
            has_required_packages: None,
            custom_checks: HashMap::new(),
            meets_requirements: false,
            warnings: vec![detail.to_string()],
            failed_policies,
            cve_checks: Vec::new(),
        }
    }

    /// Creates a policy result from Nix JSON and stable assigned-policy identities.
    ///
    /// Uses `policy_result_key(policy_id)` for CF-agent and package checks, and
    /// `rule.field_name` for multi-rule custom checks (existing convention).
    ///
    /// # Errors
    ///
    /// Returns an error when required evaluator metadata is absent, has the
    /// wrong type, or contradicts the unconditional agent result. Constituent
    /// composite evaluation errors remain contained rule outcomes.
    pub fn from_assigned(
        system_name: String,
        policies_json: &serde_json::Value,
        assigned: &[AssignedPolicy],
    ) -> Result<Self, String> {
        let mut warnings = Vec::new();
        let mut has_required_packages: Option<bool> = None;
        let mut custom_checks = HashMap::new();
        let mut failed_policies = Vec::new();
        let mut assigned_results: BTreeMap<Uuid, AssignedPolicyCheckResult> = BTreeMap::new();

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
                    // The assigned require_cf_agent policy's stable-key
                    // result and the unconditional cfAgentEnabled metadata
                    // are generated from the same underlying expression and
                    // must always agree. A generator regression that lets
                    // them diverge must not silently let the assigned
                    // result overwrite the unconditional signal — treat the
                    // mismatch as an infrastructure error instead.
                    if let Some(unconditional) = cf_agent_enabled {
                        if unconditional != value {
                            return Err(format!(
                                "Configuration {:?}: assigned CF-agent policy result \
                                 (id={}, key={:?}, value={}) disagrees with unconditional \
                                 cfAgentEnabled metadata (value={}); these must always agree",
                                system_name, ap.policy_id, key, value, unconditional,
                            ));
                        }
                    }
                    cf_agent_enabled = Some(value);
                    assigned_results.insert(
                        ap.policy_id,
                        AssignedPolicyCheckResult {
                            passed: Some(value),
                            composite_outcomes: Vec::new(),
                        },
                    );
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
                    assigned_results.insert(
                        ap.policy_id,
                        AssignedPolicyCheckResult {
                            passed: Some(value),
                            composite_outcomes: Vec::new(),
                        },
                    );
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
                                assigned_results.insert(
                                    ap.policy_id,
                                    AssignedPolicyCheckResult {
                                        passed: Some(v),
                                        composite_outcomes: Vec::new(),
                                    },
                                );
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
                        assigned_results.insert(
                            ap.policy_id,
                            AssignedPolicyCheckResult {
                                passed: Some(overall_passed),
                                composite_outcomes: Vec::new(),
                            },
                        );
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
                DeploymentPolicy::Composite { config } => {
                    let mut outcomes = Vec::new();
                    for rule in &config.rules {
                        if matches!(rule.rule, CompositeRuleKind::EvalPassed(_)) {
                            outcomes.push(CompositeRuleOutcome {
                                rule_id: rule.id,
                                kind: rule.rule.kind().to_string(),
                                phase: EnforcementPhase::Evaluation,
                                outcome: EnforcementOutcome::Pass,
                                blocking: false,
                                detail: "Configuration evaluation completed".to_string(),
                                evidence: serde_json::json!({ "configuration": system_name }),
                            });
                            continue;
                        }
                        if matches!(rule.rule, CompositeRuleKind::PinRequired(_)) {
                            let expected = policies_json
                                .get("requestedSourceRevision")
                                .and_then(|value| value.as_str())
                                .filter(|value| !value.trim().is_empty());
                            let resolved = policies_json
                                .get("resolvedSourceRevision")
                                .and_then(|value| value.as_str())
                                .filter(|value| !value.trim().is_empty());
                            let immutable = |revision: &str| {
                                matches!(revision.len(), 40 | 64)
                                    && revision.bytes().all(|byte| byte.is_ascii_hexdigit())
                            };
                            let (outcome, detail) = match (expected, resolved) {
                                (None, _) => (
                                    EnforcementOutcome::Error,
                                    "Evaluator did not preserve the requested source revision",
                                ),
                                (Some(expected), _) if !immutable(expected) => (
                                    EnforcementOutcome::Error,
                                    "Requested source revision is not a full immutable Git revision",
                                ),
                                (Some(_), None) => (
                                    EnforcementOutcome::Fail,
                                    "Nix resolved a mutable source without an immutable revision",
                                ),
                                (Some(expected), Some(resolved)) if expected != resolved => (
                                    EnforcementOutcome::Fail,
                                    "Nix resolved a different source revision than requested",
                                ),
                                (Some(_), Some(resolved)) if !immutable(resolved) => (
                                    EnforcementOutcome::Fail,
                                    "Nix resolved a non-immutable source revision",
                                ),
                                (Some(_), Some(_)) => (
                                    EnforcementOutcome::Pass,
                                    "Nix resolved the exact requested immutable source revision",
                                ),
                            };
                            outcomes.push(CompositeRuleOutcome {
                                rule_id: rule.id,
                                kind: rule.rule.kind().to_string(),
                                phase: EnforcementPhase::Evaluation,
                                outcome,
                                blocking: outcome != EnforcementOutcome::Pass,
                                detail: detail.to_string(),
                                evidence: serde_json::json!({
                                    "expected_revision": expected,
                                    "resolved_revision": resolved,
                                }),
                            });
                            continue;
                        }
                        if !matches!(
                            rule.rule,
                            CompositeRuleKind::NixosOption(_)
                                | CompositeRuleKind::PackagesInstalled(_)
                                | CompositeRuleKind::PackagesAbsent(_)
                                | CompositeRuleKind::CustomEval(_)
                        ) {
                            continue;
                        }
                        let result_key = composite_rule_result_key(&ap.policy_id, &rule.id);
                        let (outcome, detail) = match policies_json.get(&result_key) {
                            Some(value) => {
                                let success =
                                    value.get("success").and_then(|value| value.as_bool());
                                let result = value.get("value").and_then(|value| value.as_bool());
                                match (success, result) {
                                    (Some(true), Some(true)) => (
                                        EnforcementOutcome::Pass,
                                        "Rule evaluated to true".to_string(),
                                    ),
                                    (Some(true), Some(false)) => (
                                        EnforcementOutcome::Fail,
                                        "Rule evaluated to false".to_string(),
                                    ),
                                    (Some(false), _) => (
                                        EnforcementOutcome::Error,
                                        "Rule evaluation was contained by builtins.tryEval"
                                            .to_string(),
                                    ),
                                    _ => (
                                        EnforcementOutcome::Error,
                                        "Evaluator emitted malformed rule metadata".to_string(),
                                    ),
                                }
                            }
                            None => (
                                EnforcementOutcome::Error,
                                "Evaluator did not emit this rule result".to_string(),
                            ),
                        };
                        outcomes.push(CompositeRuleOutcome {
                            rule_id: rule.id,
                            kind: rule.rule.kind().to_string(),
                            phase: EnforcementPhase::Evaluation,
                            outcome,
                            blocking: outcome != EnforcementOutcome::Pass,
                            detail,
                            evidence: serde_json::json!({ "metadata_key": result_key }),
                        });
                    }
                    let aggregate = aggregate_composite_outcomes(&outcomes);
                    let passed = aggregate == EnforcementOutcome::Pass
                        || (outcomes.is_empty()
                            && config.rules.iter().all(|rule| {
                                !matches!(
                                    rule.rule,
                                    CompositeRuleKind::NixosOption(_)
                                        | CompositeRuleKind::PackagesInstalled(_)
                                        | CompositeRuleKind::PackagesAbsent(_)
                                        | CompositeRuleKind::CustomEval(_)
                                )
                            }));
                    assigned_results.insert(
                        ap.policy_id,
                        AssignedPolicyCheckResult {
                            passed: Some(passed),
                            composite_outcomes: outcomes,
                        },
                    );
                    if !passed {
                        warnings.push(format!(
                            "Composite policy {} failed during evaluation",
                            ap.policy_name
                        ));
                        failed_policies.push((ap.policy.description(), true));
                    }
                }
            }
        }

        // The Crystal Forge agent is a global, unconditional requirement: it must
        // be enabled regardless of whether an explicit require_cf_agent policy is
        // assigned to this configuration. Without this, a configuration with no
        // require_cf_agent policy assigned but cfAgentEnabled=false would report
        // meets_requirements=true from failed_policies alone, even though the
        // database gate (policy_requirements_met) independently blocks the build.
        // That mismatch let the live evaluator announce "evaluation and policies
        // passed" for a system that was never going to be queued.
        let already_recorded_agent_failure = assigned
            .iter()
            .any(|ap| matches!(ap.policy, DeploymentPolicy::RequireCrystalForgeAgent { .. }));
        if cf_agent_enabled == Some(false) && !already_recorded_agent_failure {
            warnings.push(format!(
                "Crystal Forge agent not enabled for {}",
                system_name
            ));
            failed_policies.push(("Crystal Forge agent is disabled".to_string(), true));
        }

        let meets_requirements = cf_agent_enabled == Some(true)
            && !failed_policies.iter().any(|(_, is_strict)| *is_strict);
        Ok(PolicyCheckResult {
            system_name,
            cf_agent_enabled,
            assigned_results,
            has_required_packages,
            custom_checks,
            meets_requirements,
            warnings,
            failed_policies,
            cve_checks: Vec::new(),
        })
    }

    /// Creates a policy result through the legacy flat-policy compatibility path.
    ///
    /// Composite policies cannot retain stable identities through this path and
    /// therefore produce a blocking compatibility failure.
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
            if matches!(policy, DeploymentPolicy::Composite { .. }) {
                warnings.push(format!(
                    "{}: legacy flat policy parser cannot evaluate composite policy",
                    system_name
                ));
                failed_policies.push((
                    "Legacy flat policy parser cannot evaluate composite policy".to_string(),
                    true,
                ));
                continue;
            }
            // CVE policies are not Nix-evaluated; skip here.
            if !policy.is_nix_evaluated() {
                continue;
            }

            let policy_idx = nix_policy_idx;
            nix_policy_idx += 1;

            let is_strict = policy.is_strict();

            match policy {
                DeploymentPolicy::RequireCrystalForgeAgent { .. } => {
                    let field_name = "cfAgentEnabled";
                    let value = policies_json.get(field_name).and_then(|v| v.as_bool());
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
                    let field_name = format!("hasRequiredPackages_{policy_idx}");
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
                DeploymentPolicy::Composite { .. } => {
                    unreachable!("handled before phase filtering")
                }
            }
        }

        let meets_requirements = !failed_policies.iter().any(|(_, is_strict)| *is_strict);

        PolicyCheckResult {
            system_name,
            cf_agent_enabled,
            // Legacy path has no stable policy UUIDs to key by; the matrix and
            // policy_results_json fall back to `has_required_packages`/
            // `custom_checks` for results derived from this path.
            assigned_results: BTreeMap::new(),
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

/// Returns a Nix double-quoted string without permitting interpolation.
///
/// A NUL byte produces a Nix `throw` expression because Nix strings cannot
/// represent NUL.
pub fn nix_string_pub(value: &str) -> String {
    nix_string(value)
}

fn nix_string(value: &str) -> String {
    if value.contains('\0') {
        return "throw \"Nix strings cannot represent NUL bytes\"".to_string();
    }
    let mut encoded = String::with_capacity(value.len() + 2);
    encoded.push('"');
    let mut chars = value.chars().peekable();
    while let Some(character) = chars.next() {
        match character {
            '"' => encoded.push_str("\\\""),
            '\\' => encoded.push_str("\\\\"),
            '\n' => encoded.push_str("\\n"),
            '\r' => encoded.push_str("\\r"),
            '\t' => encoded.push_str("\\t"),
            '$' if chars.peek() == Some(&'{') => encoded.push_str("\\$"),
            character if character.is_control() => {
                write!(
                    encoded,
                    "${{builtins.fromJSON \"\\\"\\\\u{:04x}\\\"\"}}",
                    character as u32
                )
                .expect("writing to a String cannot fail");
            }
            character => encoded.push(character),
        }
    }
    encoded.push('"');
    encoded
}

/// Return the stable metadata key used in the Nix expression and parsed in
/// `PolicyCheckResult::from_json` for an assigned policy with the given UUID.
pub fn policy_result_key(policy_id: &Uuid) -> String {
    // Use the first 8 hex chars of the UUID to keep keys readable and unique.
    format!("policy_{}", policy_id.to_string().replace('-', ""))
}

/// Returns the stable evaluator metadata key for one composite rule.
pub fn composite_rule_result_key(policy_id: &Uuid, rule_id: &Uuid) -> String {
    format!("composite_{}_{}", policy_id.simple(), rule_id.simple())
}

fn semantic_nix_literal(value: &serde_json::Value) -> Result<String, String> {
    match value {
        serde_json::Value::Bool(value) => Ok(value.to_string()),
        serde_json::Value::Number(value) if value.as_i64().is_some() => Ok(value.to_string()),
        serde_json::Value::String(value) => Ok(nix_string(value)),
        _ => Err("semantic value cannot be represented as a typed Nix literal".to_string()),
    }
}

fn package_identity_expression(packages: &[String], absent: bool) -> String {
    let names = packages
        .iter()
        .map(|package| nix_string(package))
        .collect::<Vec<_>>()
        .join(" ");
    if absent {
        format!(
            "builtins.all (prohibited: builtins.all (pkg: (pkg.pname or \"\") != prohibited) config.environment.systemPackages) [ {names} ]"
        )
    } else {
        format!(
            "builtins.all (required: builtins.any (pkg: (pkg.pname or \"\") == required) config.environment.systemPackages) [ {names} ]"
        )
    }
}

fn composite_evaluation_expression(rule: &CompositeRuleKind) -> Option<String> {
    let expression = match rule {
        CompositeRuleKind::NixosOption(option) => {
            let path = parse_nixos_option_path(&option.path)
                .ok()?
                .iter()
                .map(|segment| nix_string(segment))
                .collect::<Vec<_>>()
                .join(" ");
            let expected = semantic_nix_literal(&option.value).ok()?;
            format!(
                "(builtins.foldl' (current: segment: builtins.getAttr segment current) config [ {path} ]) {} {expected}",
                option.operator
            )
        }
        CompositeRuleKind::PackagesInstalled(packages) => {
            package_identity_expression(&packages.packages, false)
        }
        CompositeRuleKind::PackagesAbsent(packages) => {
            package_identity_expression(&packages.packages, true)
        }
        CompositeRuleKind::CustomEval(custom) => {
            format!("({})", custom.expression)
        }
        _ => return None,
    };
    Some(format!(
        "let attempt = builtins.tryEval ({expression}); isBoolean = attempt.success && builtins.isBool attempt.value; in {{ success = isBoolean; value = if isBoolean then attempt.value else false; }}"
    ))
}

/// Builds standalone Nix field lines for one configuration's assigned policies.
///
/// Each line uses two-space indentation. Composite rules use stable policy and
/// rule IDs so results cannot collide across policies.
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
            DeploymentPolicy::Composite { config } => {
                for rule in &config.rules {
                    if let Some(expression) = composite_evaluation_expression(&rule.rule) {
                        lines.push(format!(
                            "{}{} = {};",
                            indent,
                            composite_rule_result_key(&ap.policy_id, &rule.id),
                            expression
                        ));
                    }
                }
            }
            DeploymentPolicy::CustomCheck {
                expression,
                field_name,
                rules,
                ..
            } if rules.is_empty() => {
                // Legacy single-expression custom check: emit under the configured
                // field_name so the parser can find it. The expression is inserted
                // verbatim and must use the `config.*` lexical contract.
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
                // Expressions are expected to use `config.*` per the documented
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
                // require_packages) use `config.*` in their expression fragments
                // because the checker function receives the configuration object.
                if let Ok((_, expr)) = ap.policy.to_nix_expression_with_index(idx) {
                    lines.push(format!("{}{} = {};", indent, key, expr));
                }
            }
        }
    }

    lines
}
/// Legacy Nix prelude for configuration snapshot extraction.
///
/// This fragment is not part of [`build_nix_eval_expression`]. A future
/// targeted inspector can move the useful value and provenance logic behind a
/// separate process boundary after the primary evaluator rollback is proven in
/// production.
///
/// INVARIANT: Nix errors such as missing attributes can escape `tryEval`.
/// Therefore this fragment MUST NOT be embedded in the primary evaluator.
#[allow(
    dead_code,
    reason = "retained only as input to the post-deployment targeted inspector design"
)]
pub(crate) const SNAPSHOT_EXTRACTION_PRELUDE: &str = r#"
  min = left: right: if left < right then left else right;
  take = count: values: builtins.genList
    (index: builtins.elemAt values index) (min count (builtins.length values));
  safeValue = depth: raw:
    let
      # Classify weak-head-normal-form first. Collection elements are forced
      # only after the bounded prefix is selected, so an omitted element cannot
      # poison retained values.
      typeAttempt = builtins.tryEval (builtins.typeOf raw);
      valueType = typeAttempt.value or "failed";
      scalarAttempt = builtins.tryEval (builtins.deepSeq raw raw);
      scalar = scalarAttempt.value or null;
      namesAttempt = if valueType == "set"
        then builtins.tryEval (builtins.attrNames raw)
        else { success = false; value = []; };
      limitedNames = take 100 (namesAttempt.value or []);
      lengthAttempt = if valueType == "list"
        then builtins.tryEval (builtins.length raw)
        else { success = false; value = 0; };
    in
      if !typeAttempt.success then {
        kind = "failed";
        value = { code = "not_evaluated"; message = "Option value did not evaluate"; };
      } else if builtins.elem valueType [ "null" "bool" "int" "float" "string" ]
        && !scalarAttempt.success then {
        kind = "failed";
        value = { code = "not_evaluated"; message = "Option scalar did not evaluate"; };
      } else if builtins.elem valueType [ "null" "bool" "int" "float" "string" ] then {
        kind = "scalar"; value = scalar;
      } else if valueType == "path" then {
        kind = "scalar"; value = builtins.toString raw;
      } else if valueType == "lambda" then {
        kind = "opaque"; value = { type_name = "lambda"; };
      } else if depth >= 4 then {
        kind = "opaque"; value = { type_name = valueType; };
      } else if valueType == "list" && lengthAttempt.success then {
        # A prefix is not a truthful representation of the option value. Mark
        # an over-limit collection opaque instead of serializing silent loss.
        kind = if lengthAttempt.value > 100 then "opaque" else "list";
        value = if lengthAttempt.value > 100
          then { type_name = "list_over_limit"; }
          else builtins.genList
            (index: safeValue (depth + 1) (builtins.elemAt raw index))
            lengthAttempt.value;
      } else if valueType == "list" then {
        kind = "failed";
        value = { code = "not_evaluated"; message = "Option list length did not evaluate"; };
      } else if valueType == "set" && (raw.type or null) == "derivation" then {
        kind = "package";
        value = {
          name = (builtins.tryEval (raw.name or null)).value or null;
          pname = (builtins.tryEval (raw.pname or null)).value or null;
          version = (builtins.tryEval (raw.version or null)).value or null;
          output_path = (builtins.tryEval
            (if raw ? outPath then builtins.toString raw.outPath else null)).value or null;
        };
      } else if valueType == "set" && namesAttempt.success then {
        kind = if builtins.length (namesAttempt.value or []) > 100
          then "opaque" else "attribute_set";
        value = if builtins.length (namesAttempt.value or []) > 100
          then { type_name = "attribute_set_over_limit"; }
          else builtins.listToAttrs (map (key: {
            name = key; value = safeValue (depth + 1) raw.${key};
          }) limitedNames);
      } else if valueType == "set" then {
        kind = "failed";
        value = { code = "not_evaluated"; message = "Option attribute names did not evaluate"; };
      } else {
        kind = "opaque"; value = { type_name = valueType; };
      };
  # This guard prevents cyclic or recursively generated attrsets from
  # exhausting the evaluator. It marks omitted subtrees instead of limiting
  # the number of ordinary options in the snapshot.
  optionTraversalDepthLimit = 16;
  # INVARIANT: walkOptions never propagates an evaluation error to its caller.
  #
  # Forcing an option declaration can throw. The common case is the same option
  # declared by two modules: the module system defers that throw into the
  # merged `options` node, so `config` and `system.build.toplevel` still
  # evaluate successfully while the matching option metadata stays poisoned.
  # Snapshot observability must not convert that latent condition into a system
  # evaluation failure, so every forcing point below is guarded and an
  # uninspectable node becomes explicit data instead of an abort.
  walkOptions = depth: prefix: attrs:
    let
      namesAttempt = builtins.tryEval (builtins.attrNames attrs);
    in
    if !namesAttempt.success then [ { path = prefix; unreadable = true; } ]
    # A recursive option tree must remain bounded, but reaching the guard is
    # data rather than absence. Emit one explicit failed subtree marker so the
    # persisted snapshot cannot silently claim completeness.
    else if depth >= optionTraversalDepthLimit then
      if namesAttempt.value == [] then [] else [ {
        path = prefix;
        over_depth = true;
      } ]
    else builtins.concatLists (map (name:
    let
      path = prefix ++ [ name ];
      # Weak-head-normal-form is the first forcing point a poisoned option
      # declaration reaches, so it is guarded before any inspection.
      currentAttempt = builtins.tryEval
        (let child = attrs.${name}; in builtins.seq child child);
      current = currentAttempt.value;
      # `_type` is a separate forcing point. It can throw even when the node
      # itself resolved to an attribute set.
      kindAttempt = if !currentAttempt.success
        then { success = false; value = null; }
        else builtins.tryEval
          (if builtins.isAttrs current then (current._type or null) else null);
    in
      if !currentAttempt.success || !kindAttempt.success then
        [ { inherit path; unreadable = true; } ]
      else if builtins.isAttrs current && kindAttempt.value == "option" then
        [ { inherit path; option = current; } ]
      else if builtins.isAttrs current then walkOptions (depth + 1) path current
      else []
  ) namesAttempt.value);
  optionSnapshot = lib: inputOrigins: rawModules: item:
    # A node that traversal could not inspect is represented explicitly.
    # Omitting it would let the snapshot state that the option does not exist,
    # which is a different and false claim.
    if item.unreadable or false then {
      path = builtins.concatStringsSep "." item.path;
      declared_type = "unknown";
      value = {
        kind = "failed";
        value = {
          code = "not_evaluated";
          message = "Option declaration could not be inspected";
        };
      };
      definitions = [];
      overridden = false;
    } else if item.over_depth or false then {
      path = builtins.concatStringsSep "." item.path;
      declared_type = "unknown";
      value = {
        kind = "failed";
        value = {
          code = "over_depth";
          message = "Option subtree exceeds the traversal depth limit";
        };
      };
      definitions = [];
      overridden = false;
    } else let
      path = builtins.concatStringsSep "." item.path;
      option = item.option;
      declaredType = option.type.description or (option.type.name or "unknown");
      winningDefinitions = map (definition:
        let
          sourcePath = builtins.toString (definition.file or "untracked");
          sourceInputs = builtins.filter
            (inputName:
              let origin = inputOrigins.${inputName};
              in origin.path != null && lib.hasPrefix origin.path sourcePath)
            (builtins.attrNames inputOrigins);
          sourceInput = if sourceInputs == [] then null else builtins.head sourceInputs;
        in {
          source_path = sourcePath;
          source_input = sourceInput;
          source_revision = if sourceInput == null then null else inputOrigins.${sourceInput}.revision;
          value = safeValue 0 (definition.value or null);
          # definitionsWithLocations contains the definitions that participate
          # in the final module-system merge. Discarded mkOverride values are
          # not exposed and therefore are not fabricated as provenance.
          winning = true;
          priority = option.highestPrio or null;
          status = "winning";
          winner_note = "This definition participates in the final module-system merge.";
        }) (option.definitionsWithLocations or []);
      rawDefinitions = builtins.filter (definition: definition != null) (map (module:
        let
          absent = { _crystalForgeMissing = true; };
          present = lib.hasAttrByPath item.path (module.config or {});
          raw = lib.attrByPath item.path absent (module.config or {});
           attempted = builtins.tryEval raw;
          forced = attempted.value or absent;
          priority = if builtins.isAttrs forced && (forced._type or null) == "override"
            then forced.priority else 100;
          sourcePath = builtins.toString (module._file or "untracked");
          sourceInputs = builtins.filter
            (inputName:
              let origin = inputOrigins.${inputName};
              in origin.path != null && lib.hasPrefix origin.path sourcePath)
            (builtins.attrNames inputOrigins);
          sourceInput = if sourceInputs == [] then null else builtins.head sourceInputs;
        in if !present || priority <= (option.highestPrio or 100) then null else {
          source_path = sourcePath;
          source_input = sourceInput;
          source_revision = if sourceInput == null then null else inputOrigins.${sourceInput}.revision;
          value = safeValue 0 raw;
          winning = false;
          inherit priority;
          status = "overridden";
          winner_note = "A lower numeric module-system priority won.";
        }) rawModules);
      definitions = winningDefinitions ++ rawDefinitions;
    in {
      inherit path definitions;
      declared_type = declaredType;
      value =
        if lib.hasInfix "submodule" declaredType then
          let rendered = safeValue 0 option.value;
          in if rendered.kind == "attribute_set" then rendered // { kind = "submodule"; } else rendered
        else safeValue 0 option.value;
      overridden = rawDefinitions != [];
    };
  safeOptionSnapshot = lib: inputOrigins: rawModules: item:
    let
      path = builtins.concatStringsSep "." item.path;
      snapshot = optionSnapshot lib inputOrigins rawModules item;
      attempted = builtins.tryEval (builtins.deepSeq snapshot snapshot);
    in attempted.value or {
      inherit path;
      declared_type = "unknown";
      value = {
        kind = "failed";
        value = { code = "not_evaluated"; message = "Option snapshot did not evaluate"; };
      };
      definitions = [];
      overridden = false;
    };
"#;

/// Build the complete Nix expression for `nix-eval-jobs` with per-configuration
/// policy checks derived from the `PoliciesByConfiguration` map.
///
/// Each `nixosConfigurations.<name>` output is checked only against the
/// policies assigned to the Crystal Forge system(s) for that configuration.
/// Configurations that are unregistered or have no assigned policies receive
/// only the unconditional `cfAgentEnabled` metadata.
///
/// INVARIANT: This primary evaluator must not inspect configuration options,
/// module graphs, or exported modules. Those observability concerns have a
/// different failure boundary. Expanding this expression's search space can
/// turn a lazy metadata defect into a failed system evaluation and prevent an
/// otherwise valid derivation from being built.
///
/// The expression structure:
/// ```nix
/// (import ./primary_evaluation.nix) {
///   flakeRef = "<flakeRef>";
///   policyCheckers = {
///     "<config>" = config: { policy_<id> = <expr>; ... };
///     ...
///   };
///   requestedRevision = "<full revision>";
/// }
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
            format!("        {} = config: {};", nix_string(config_name), body)
        })
        .collect();

    let checkers_block = if checker_entries.is_empty() {
        "{}".to_string()
    } else {
        format!("{{\n{}\n      }}", checker_entries.join("\n"))
    };

    let requested_revision =
        nix_string(crate::derivations::utils::flake_reference_revision(flake_ref).unwrap_or(""));
    format!(
        "({}) {{ flakeRef = {}; policyCheckers = {}; requestedRevision = {}; }}",
        include_str!("primary_evaluation.nix"),
        nix_string(flake_ref),
        checkers_block,
        requested_revision,
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
    /// Number of trusted/eligible policy_requirement_mappings for this policy version
    #[sqlx(skip)]
    #[serde(default)]
    pub mapped_requirement_count: i64,
    /// Number of distinct bundle lineages using this policy version
    #[sqlx(skip)]
    #[serde(default)]
    pub bundle_usage_count: i64,
}

/// Request to create a new deployment policy
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CreateDeploymentPolicyRequest {
    pub name: String,
    pub description: Option<String>,
    pub policy_type: String,
    pub config: serde_json::Value,
    pub enabled: Option<bool>,
    /// Security Requirements Guide IDs this control satisfies.
    /// Normalised and validated server-side; stored in compliance_metadata.
    #[serde(default)]
    pub srg_ids: Vec<String>,
    /// Control Correlation Identifier mappings.
    /// Normalised and validated server-side; stored in compliance_metadata.
    #[serde(default)]
    pub cci_ids: Vec<String>,
    /// Policy category: "deployment", "pipeline", "rollout", "security"
    #[serde(default)]
    pub category: Option<String>,
    /// Framework string, e.g. "DISA STIG", "NIST 800-53", "CMMC 2.0", "CIS Benchmark", or custom
    #[serde(default)]
    pub framework: Option<String>,
    /// Severity: "high", "medium", "low"
    #[serde(default)]
    pub severity: Option<String>,
    /// NIST 800-53 control family, e.g. "AC", "AU", "CM", "IA", "SC", "SI", "MP"
    #[serde(default)]
    pub control_family: Option<String>,
    /// CMMC 2.0 maturity level: 1, 2, or 3
    #[serde(default)]
    pub cmmc_level: Option<i32>,
    /// CIS Benchmark section, e.g. "5.2.3"
    #[serde(default)]
    pub cis_section: Option<String>,
    /// Human-readable rationale for this control
    #[serde(default)]
    pub rationale: Option<String>,
    /// Evidence collection specifications for ATO audits
    #[serde(default)]
    pub evidence_specs: Vec<crate::api::models::EvidenceSpec>,
    /// Normalized requirement mappings to persist with the initial draft.
    #[serde(default)]
    pub requirement_mappings: Vec<CreatePolicyRequirementMapping>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreatePolicyRequirementMapping {
    pub requirement_version_id: uuid::Uuid,
    pub relationship: String,
    pub coverage: String,
    pub rationale: Option<String>,
    #[serde(default = "default_mapping_provenance")]
    pub provenance: String,
}

fn default_mapping_provenance() -> String {
    "manual".to_string()
}

/// Request to update an existing deployment policy
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdateDeploymentPolicyRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub policy_type: Option<String>,
    pub config: Option<serde_json::Value>,
    pub enabled: Option<bool>,
    /// When `Some`, replace the curated SRG mapping; `Some([])` clears it.
    /// When `None`, the existing value is preserved.
    #[serde(default)]
    pub srg_ids: Option<Vec<String>>,
    /// When `Some`, replace the curated CCI mapping; `Some([])` clears it.
    /// When `None`, the existing value is preserved.
    #[serde(default)]
    pub cci_ids: Option<Vec<String>>,
    /// When `Some`, replace the category; `None` preserves existing.
    #[serde(default)]
    pub category: Option<String>,
    /// When `Some(Some(_))`, replaces the framework. `Some(None)` clears it.
    /// `None` preserves the existing value.
    #[serde(default)]
    pub framework: Option<Option<String>>,
    /// When `Some(Some(_))`, replaces the severity. `Some(None)` clears it.
    /// `None` preserves the existing value.
    #[serde(default)]
    pub severity: Option<Option<String>>,
    /// When `Some(Some(_))`, replaces the control family. `Some(None)` clears it.
    /// `None` preserves the existing value.
    #[serde(default)]
    pub control_family: Option<Option<String>>,
    /// When `Some(Some(_))`, replaces the CMMC level. `Some(None)` clears it.
    /// `None` preserves the existing value.
    #[serde(default)]
    pub cmmc_level: Option<Option<i32>>,
    /// When `Some(Some(_))`, replaces the CIS section. `Some(None)` clears it.
    /// `None` preserves the existing value.
    #[serde(default)]
    pub cis_section: Option<Option<String>>,
    /// When `Some(Some(_))`, replaces the rationale. `Some(None)` clears it.
    /// `None` preserves the existing value.
    #[serde(default)]
    pub rationale: Option<Option<String>>,
    /// When `Some`, replace evidence specs; `Some([])` clears them.
    /// When `None`, the existing value is preserved.
    #[serde(default)]
    pub evidence_specs: Option<Vec<crate::api::models::EvidenceSpec>>,
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn nix_eval_json(expression: &str) -> serde_json::Value {
        let output = std::process::Command::new("nix")
            .args([
                "--extra-experimental-features",
                "nix-command",
                "--store",
                "dummy://",
                "eval",
                "--json",
                "--expr",
                expression,
            ])
            .output()
            .expect("failed to spawn nix eval");
        assert!(
            output.status.success(),
            "nix eval failed:\n{}\nExpression:\n{}",
            String::from_utf8_lossy(&output.stderr),
            expression
        );
        serde_json::from_slice(&output.stdout).expect("nix eval output must be JSON")
    }

    fn composite_config() -> serde_json::Value {
        serde_json::json!({
            "schema_version": 1,
            "mode": "all",
            "rules": [
                {
                    "id": "10000000-0000-0000-0000-000000000001",
                    "kind": "nixos_option",
                    "config": {
                        "path": "networking.firewall.enable",
                        "operator": "==",
                        "value_type": "boolean",
                        "value": true
                    }
                },
                {
                    "id": "10000000-0000-0000-0000-000000000002",
                    "kind": "packages_installed",
                    "config": { "packages": ["openssh"] }
                },
                {
                    "id": "10000000-0000-0000-0000-000000000003",
                    "kind": "custom_eval",
                    "config": { "expression": "config.security.audit.enable", "message": "audit" }
                },
                {
                    "id": "10000000-0000-0000-0000-000000000004",
                    "kind": "cve_block",
                    "config": { "severity": "critical", "max_allowed": 0 }
                },
                {
                    "id": "10000000-0000-0000-0000-000000000005",
                    "kind": "packages_absent",
                    "config": { "packages": ["telnet"] }
                },
                {
                    "id": "10000000-0000-0000-0000-000000000006",
                    "kind": "eval_passed",
                    "config": {}
                },
                {
                    "id": "10000000-0000-0000-0000-000000000007",
                    "kind": "pin_required",
                    "config": {}
                },
                {
                    "id": "10000000-0000-0000-0000-000000000008",
                    "kind": "time_window",
                    "config": {
                        "days": ["mon", "tue"],
                        "from": "09:00",
                        "to": "17:00",
                        "tz": "America/New_York"
                    }
                }
            ]
        })
    }

    fn single_composite_rule(kind: &str, config: serde_json::Value) -> serde_json::Value {
        serde_json::json!({
            "schema_version": 1,
            "mode": "all",
            "rules": [{
                "id": "11000000-0000-0000-0000-000000000001",
                "kind": kind,
                "config": config
            }]
        })
    }

    #[test]
    fn composite_round_trip_preserves_ids_order_kinds_and_semantic_values() {
        let parsed = validate_policy_type_config(COMPOSITE_POLICY_TYPE, &composite_config())
            .unwrap()
            .unwrap();
        let serialized = serde_json::to_value(&parsed).unwrap();

        assert_eq!(serialized, composite_config());
        assert_eq!(parsed.rules.len(), 8);
        assert!(matches!(
            parsed.rules[0].rule,
            CompositeRuleKind::NixosOption(NixosOptionRuleConfig {
                value: serde_json::Value::Bool(true),
                ..
            })
        ));
    }

    #[test]
    fn nixos_option_path_parser_preserves_quoted_dots_and_escapes() {
        assert_eq!(
            parse_nixos_option_path(r#"boot.kernel.sysctl."kernel.randomize_va_space""#).unwrap(),
            ["boot", "kernel", "sysctl", "kernel.randomize_va_space"]
        );
        assert_eq!(
            parse_nixos_option_path(r#"environment.etc."issue".text"#).unwrap(),
            ["environment", "etc", "issue", "text"]
        );
        assert_eq!(
            parse_nixos_option_path(r#"services."quoted\"name\\suffix".enable"#).unwrap(),
            ["services", "quoted\"name\\suffix", "enable"]
        );
        for invalid in [
            "",
            "a..b",
            ".a",
            "a.",
            "a.\"unterminated",
            "a.\"\".b",
            "a. b",
        ] {
            assert!(parse_nixos_option_path(invalid).is_err(), "{invalid:?}");
        }
    }

    #[test]
    fn composite_validation_rejects_non_pname_package_contract_and_bad_custom_syntax() {
        let mut invalid_package = composite_config();
        invalid_package["rules"][1]["config"]["packages"] = serde_json::json!(["nixpkgs#openssh"]);
        assert!(
            validate_policy_type_config(COMPOSITE_POLICY_TYPE, &invalid_package)
                .unwrap_err()
                .contains("invalid pname")
        );

        let mut malformed = composite_config();
        malformed["rules"][2]["config"]["expression"] =
            serde_json::json!("true); injected = true; (");
        assert!(
            validate_policy_type_config(COMPOSITE_POLICY_TYPE, &malformed)
                .unwrap_err()
                .contains("invalid Nix expression syntax")
        );
    }

    #[tokio::test]
    async fn async_policy_validation_matches_sync_semantics_without_panicking() {
        let valid = composite_config();
        let mut malformed = composite_config();
        malformed["rules"][2]["config"]["expression"] =
            serde_json::json!("true); injected = true; (");

        for (policy_type, config) in [
            (COMPOSITE_POLICY_TYPE, &valid),
            (COMPOSITE_POLICY_TYPE, &malformed),
            (
                "require_packages",
                &serde_json::json!({"packages": ["curl"]}),
            ),
        ] {
            let synchronous = validate_policy_type_config(policy_type, config);
            let asynchronous = validate_policy_type_config_async(policy_type, config).await;
            assert_eq!(asynchronous, synchronous);
        }
    }

    #[test]
    fn runtime_composite_deserialization_never_requires_nix_parser_syntax_validation() {
        let config = serde_json::json!({
            "schema_version": 1,
            "mode": "all",
            "rules": [{
                "id": "30000000-0000-0000-0000-000000000099",
                "kind": "custom_eval",
                "config": {"expression": "config: (", "message": "malformed"}
            }]
        });

        assert!(deserialize_policy_type_config(COMPOSITE_POLICY_TYPE, &config).is_ok());
        assert!(validate_policy_type_config(COMPOSITE_POLICY_TYPE, &config).is_err());

        let mut oversized = config;
        oversized["rules"][0]["config"]["expression"] =
            serde_json::Value::String("x".repeat(MAX_CUSTOM_EVAL_EXPRESSION_BYTES + 1));
        assert!(deserialize_policy_type_config(COMPOSITE_POLICY_TYPE, &oversized).is_err());
    }

    #[test]
    #[ignore = "requires Nix evaluator in PATH"]
    fn quoted_option_paths_and_safe_strings_evaluate_authoritatively_in_nix() {
        let policy_id = Uuid::from_u128(50);
        let randomize_id = Uuid::from_u128(51);
        let issue_id = Uuid::from_u128(52);
        let assigned = AssignedPolicy {
            policy_id,
            policy_name: "quoted paths".to_string(),
            policy: DeploymentPolicy::Composite {
                config: CompositePolicyConfig {
                    schema_version: 1,
                    mode: CompositeRuleMode::All,
                    rules: vec![
                        CompositeRule {
                            id: randomize_id,
                            rule: CompositeRuleKind::NixosOption(NixosOptionRuleConfig {
                                path: r#"boot.kernel.sysctl."kernel.randomize_va_space""#
                                    .to_string(),
                                operator: "==".to_string(),
                                value_type: NixosOptionValueType::Integer,
                                value: serde_json::json!(2),
                            }),
                        },
                        CompositeRule {
                            id: issue_id,
                            rule: CompositeRuleKind::NixosOption(NixosOptionRuleConfig {
                                path: r#"environment.etc."issue".text"#.to_string(),
                                operator: "==".to_string(),
                                value_type: NixosOptionValueType::Lines,
                                value: serde_json::json!(
                                    "Authorized ${literal}\nLine\r\t\\\"\u{1}"
                                ),
                            }),
                        },
                    ],
                },
            },
        };
        let fields = build_policy_fields_for_config_standalone(&[assigned]).join("\n");
        let encoded_issue = nix_string("Authorized ${literal}\nLine\r\t\\\"\u{1}");
        let expression = format!(
            r#"let
  config = {{
    boot.kernel.sysctl."kernel.randomize_va_space" = 2;
    environment.etc."issue".text = {encoded_issue};
  }};
in {{ {fields} }}"#
        );
        let evaluated = nix_eval_json(&expression);
        for rule_id in [randomize_id, issue_id] {
            let result = &evaluated[composite_rule_result_key(&policy_id, &rule_id)];
            assert_eq!(result["success"], true);
            assert_eq!(result["value"], true);
        }
    }

    #[test]
    #[ignore = "requires Nix evaluator in PATH"]
    fn nix_string_encoder_round_trips_interpolation_and_controls() {
        let values = [
            "quote \" slash \\",
            "literal ${builtins.abort \"injected\"}",
            "line\ncarriage\rtab\t",
            "controls \u{1}\u{8}\u{1f}",
        ];
        let expression = format!(
            "[ {} ]",
            values
                .iter()
                .map(|value| nix_string(value))
                .collect::<Vec<_>>()
                .join(" ")
        );
        assert_eq!(nix_eval_json(&expression), serde_json::json!(values));

        let nul = nix_string("contains\0nul");
        let output = std::process::Command::new("nix")
            .args(["eval", "--json", "--expr", &nul])
            .output()
            .expect("failed to spawn nix eval");
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("cannot represent NUL"));
    }

    #[test]
    #[ignore = "requires Nix evaluator in PATH"]
    fn malformed_custom_eval_cannot_break_or_inject_generated_expression() {
        let policy_id = Uuid::from_u128(60);
        let rules = [
            (Uuid::from_u128(61), "true"),
            (Uuid::from_u128(62), "throw \"runtime\""),
            (Uuid::from_u128(63), "42"),
            (Uuid::from_u128(64), "true); injected = true; ("),
        ];
        let assigned = AssignedPolicy {
            policy_id,
            policy_name: "isolated custom expressions".to_string(),
            policy: DeploymentPolicy::Composite {
                config: CompositePolicyConfig {
                    schema_version: 1,
                    mode: CompositeRuleMode::All,
                    rules: rules
                        .iter()
                        .map(|(id, expression)| CompositeRule {
                            id: *id,
                            rule: CompositeRuleKind::CustomEval(CustomEvalRuleConfig {
                                expression: expression.to_string(),
                                message: "test".to_string(),
                            }),
                        })
                        .collect(),
                },
            },
        };
        let fields = build_policy_fields_for_config_standalone(&[assigned]).join("\n");
        assert!(!fields.contains("injected"));
        let evaluated = nix_eval_json(&format!("let config = {{}}; in {{ {fields} }}"));
        assert_eq!(
            evaluated[composite_rule_result_key(&policy_id, &rules[0].0)]["success"],
            true
        );
        for (id, _) in &rules[1..] {
            assert_eq!(
                evaluated[composite_rule_result_key(&policy_id, id)]["success"],
                false
            );
        }
    }

    #[test]
    fn composite_validation_rejects_empty_nil_duplicate_unknown_and_type_mismatches() {
        let mut cases = Vec::new();

        let mut wrong_schema = composite_config();
        wrong_schema["schema_version"] = serde_json::json!(2);
        cases.push(wrong_schema);

        let mut unsupported_mode = composite_config();
        unsupported_mode["mode"] = serde_json::json!("any");
        cases.push(unsupported_mode);

        let mut malformed_id = composite_config();
        malformed_id["rules"][0]["id"] = serde_json::json!("not-a-uuid");
        cases.push(malformed_id);

        let mut empty = composite_config();
        empty["rules"] = serde_json::json!([]);
        cases.push(empty);

        let mut nil = composite_config();
        nil["rules"][0]["id"] = serde_json::json!(Uuid::nil());
        cases.push(nil);

        let mut duplicate = composite_config();
        duplicate["rules"][1]["id"] = duplicate["rules"][0]["id"].clone();
        cases.push(duplicate);

        let mut unknown = composite_config();
        unknown["rules"][0]["kind"] = serde_json::json!("approval_required");
        cases.push(unknown);

        let mut wrong_value = composite_config();
        wrong_value["rules"][0]["config"]["value"] = serde_json::json!("true");
        cases.push(wrong_value);

        let mut wrong_operator = composite_config();
        wrong_operator["rules"][0]["config"]["operator"] = serde_json::json!(">=");
        cases.push(wrong_operator);

        for config in cases {
            assert!(validate_policy_type_config(COMPOSITE_POLICY_TYPE, &config).is_err());
        }
    }

    #[test]
    fn composite_validation_enforces_conservative_rule_limit() {
        let rule = single_composite_rule("eval_passed", serde_json::json!({}))["rules"][0].clone();
        let mut at_limit = single_composite_rule("eval_passed", serde_json::json!({}));
        at_limit["rules"] = serde_json::Value::Array(
            (0..MAX_COMPOSITE_RULES)
                .map(|index| {
                    let mut rule = rule.clone();
                    rule["id"] = serde_json::json!(Uuid::from_u128(index as u128 + 1));
                    rule
                })
                .collect(),
        );
        assert!(validate_policy_type_config(COMPOSITE_POLICY_TYPE, &at_limit).is_ok());

        at_limit["rules"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!({
                "id": Uuid::from_u128(MAX_COMPOSITE_RULES as u128 + 1),
                "kind": "eval_passed",
                "config": {}
            }));
        let error = validate_policy_type_config(COMPOSITE_POLICY_TYPE, &at_limit).unwrap_err();
        assert!(error.contains("at most 64 rules"), "{error}");
    }

    #[test]
    fn composite_single_expression_helper_returns_explicit_error() {
        let config = validate_policy_type_config(COMPOSITE_POLICY_TYPE, &composite_config())
            .unwrap()
            .unwrap();
        let policy = DeploymentPolicy::Composite { config };

        assert!(policy.is_nix_evaluated());
        assert_eq!(
            policy.to_nix_expression().unwrap_err(),
            "composite policies require phase-specific per-rule evaluation"
        );
        assert_eq!(
            policy.field_name().unwrap_err(),
            "composite policies use stable per-rule field keys"
        );
    }

    #[test]
    fn ac3_validation_matrix_accepts_and_rejects_each_exposed_kind_discriminately() {
        let cases = [
            (
                "nixos_option",
                serde_json::json!({"path": "networking.firewall.enable", "operator": "==", "value_type": "boolean", "value": true}),
                serde_json::json!({"path": "networking.firewall.enable", "operator": ">", "value_type": "boolean", "value": true}),
            ),
            (
                "packages_installed",
                serde_json::json!({"packages": ["openssh"]}),
                serde_json::json!({"packages": []}),
            ),
            (
                "packages_absent",
                serde_json::json!({"packages": ["telnet"]}),
                serde_json::json!({"packages": ["nixpkgs#telnet"]}),
            ),
            (
                "custom_eval",
                serde_json::json!({"expression": "config.networking.firewall.enable", "message": "firewall"}),
                serde_json::json!({"expression": "true); injected = true; (", "message": "invalid"}),
            ),
            (
                "cve_block",
                serde_json::json!({"severity": "critical", "max_allowed": 0}),
                serde_json::json!({"severity": "catastrophic", "max_allowed": 0}),
            ),
            (
                "eval_passed",
                serde_json::json!({}),
                serde_json::json!({"unexpected": true}),
            ),
            (
                "pin_required",
                serde_json::json!({}),
                serde_json::json!({"revision": "main"}),
            ),
            (
                "time_window",
                serde_json::json!({"days": ["mon"], "from": "09:00", "to": "17:00", "tz": "UTC"}),
                serde_json::json!({"days": ["funday"], "from": "09:00", "to": "17:00", "tz": "UTC"}),
            ),
        ];

        for (kind, valid, invalid) in cases {
            let parsed = validate_policy_type_config(
                COMPOSITE_POLICY_TYPE,
                &single_composite_rule(kind, valid),
            )
            .unwrap_or_else(|error| panic!("AC3 validate/pass [{kind}]: {error}"));
            assert_eq!(parsed.unwrap().rules[0].rule.kind(), kind);
            assert!(
                validate_policy_type_config(
                    COMPOSITE_POLICY_TYPE,
                    &single_composite_rule(kind, invalid),
                )
                .is_err(),
                "AC3 validate/reject [{kind}]"
            );
        }
    }

    #[test]
    #[ignore = "requires Nix evaluator in PATH"]
    fn ac3_actual_nix_executor_matrix_distinguishes_pass_fail_error_and_evidence() {
        let policy_id = Uuid::from_u128(0x1200);
        let cases = vec![
            (
                "nixos_option",
                serde_json::json!({"path": "networking.firewall.enable", "operator": "==", "value_type": "boolean", "value": true}),
                EnforcementOutcome::Pass,
            ),
            (
                "nixos_option",
                serde_json::json!({"path": "networking.firewall.enable", "operator": "==", "value_type": "boolean", "value": false}),
                EnforcementOutcome::Fail,
            ),
            (
                "packages_installed",
                serde_json::json!({"packages": ["openssh"]}),
                EnforcementOutcome::Pass,
            ),
            (
                "packages_installed",
                serde_json::json!({"packages": ["telnet"]}),
                EnforcementOutcome::Fail,
            ),
            (
                "packages_absent",
                serde_json::json!({"packages": ["telnet"]}),
                EnforcementOutcome::Pass,
            ),
            (
                "packages_absent",
                serde_json::json!({"packages": ["openssh"]}),
                EnforcementOutcome::Fail,
            ),
            (
                "custom_eval",
                serde_json::json!({"expression": "config.networking.firewall.enable", "message": "pass"}),
                EnforcementOutcome::Pass,
            ),
            (
                "custom_eval",
                serde_json::json!({"expression": "!config.networking.firewall.enable", "message": "fail"}),
                EnforcementOutcome::Fail,
            ),
            (
                "custom_eval",
                serde_json::json!({"expression": "42", "message": "non-boolean error"}),
                EnforcementOutcome::Error,
            ),
        ];
        let mut rules = Vec::new();
        let mut expected = Vec::new();
        for (index, (kind, config, outcome)) in cases.into_iter().enumerate() {
            let id = Uuid::from_u128(0x1300 + index as u128);
            let parsed = validate_policy_type_config(
                COMPOSITE_POLICY_TYPE,
                &single_composite_rule(kind, config),
            )
            .unwrap()
            .unwrap();
            rules.push(CompositeRule {
                id,
                rule: parsed.rules.into_iter().next().unwrap().rule,
            });
            expected.push((kind, id, outcome));
        }
        let assigned = AssignedPolicy {
            policy_id,
            policy_name: "AC3 actual executor matrix".to_string(),
            policy: DeploymentPolicy::Composite {
                config: CompositePolicyConfig {
                    schema_version: 1,
                    mode: CompositeRuleMode::All,
                    rules,
                },
            },
        };
        let fields = build_policy_fields_for_config_standalone(std::slice::from_ref(&assigned));
        let metadata = nix_eval_json(&format!(
            r#"let config = {{ networking.firewall.enable = true; environment.systemPackages = [ {{ pname = "openssh"; }} ]; }}; in {{ cfAgentEnabled = true; {} }}"#,
            fields.join("\n")
        ));
        let check = PolicyCheckResult::from_assigned(
            "matrix-host".to_string(),
            &metadata,
            std::slice::from_ref(&assigned),
        )
        .expect("actual Nix metadata must parse");
        let persisted = policy_results_json(&check, std::slice::from_ref(&assigned));
        let outcomes = &check.assigned_results[&policy_id].composite_outcomes;
        for (kind, id, expected_outcome) in &expected {
            let actual = outcomes
                .iter()
                .find(|outcome| outcome.rule_id == *id)
                .unwrap();
            assert_eq!(actual.kind, *kind, "AC3 actual executor/kind [{kind}]");
            assert_eq!(
                actual.outcome, *expected_outcome,
                "AC3 actual executor/{expected_outcome:?} [{kind}]"
            );
            assert_eq!(actual.phase, EnforcementPhase::Evaluation);
            assert!(actual.evidence["metadata_key"].as_str().is_some());
            assert_eq!(
                actual.blocking,
                *expected_outcome != EnforcementOutcome::Pass
            );
        }
        assert_eq!(
            persisted["assigned"][policy_id.to_string()]["rule_outcomes"]
                .as_array()
                .unwrap()
                .len(),
            expected.len(),
            "AC3 normalized evaluator evidence"
        );

        for kind in [
            "nixos_option",
            "packages_installed",
            "packages_absent",
            "custom_eval",
        ] {
            let (_, id, _) = expected
                .iter()
                .find(|(candidate, _, _)| *candidate == kind)
                .unwrap();
            let mut missing = metadata.clone();
            missing
                .as_object_mut()
                .unwrap()
                .remove(&composite_rule_result_key(&policy_id, id));
            let missing_check = PolicyCheckResult::from_assigned(
                "matrix-host".to_string(),
                &missing,
                std::slice::from_ref(&assigned),
            )
            .expect("missing rule evidence is a contained per-rule Error");
            let missing_outcome = missing_check.assigned_results[&policy_id]
                .composite_outcomes
                .iter()
                .find(|outcome| outcome.rule_id == *id)
                .unwrap();
            assert_eq!(
                missing_outcome.outcome,
                EnforcementOutcome::Error,
                "AC3 error/missing evidence [{kind}]"
            );
            assert_eq!(missing_outcome.phase, EnforcementPhase::Evaluation);
            assert!(missing_outcome.blocking);
            assert_eq!(
                missing_outcome.evidence["metadata_key"],
                composite_rule_result_key(&policy_id, id),
                "AC3 missing-evidence identity [{kind}]"
            );
        }
    }

    #[test]
    fn nixos_option_semantic_types_and_operators_are_typed() {
        let cases = [
            ("boolean", serde_json::json!(false), "!="),
            ("enum", serde_json::json!("nftables"), "=="),
            ("integer", serde_json::json!(-9), ">="),
            ("string", serde_json::json!("short"), "=="),
            ("lines", serde_json::json!("line one\n\nline three\n"), "!="),
            ("unknown", serde_json::json!("${config.foo}\\n"), "=="),
        ];
        for (value_type, value, operator) in cases {
            let config = serde_json::json!({
                "schema_version": 1,
                "mode": "all",
                "rules": [{
                    "id": "10000000-0000-0000-0000-000000000001",
                    "kind": "nixos_option",
                    "config": {
                        "path": "services.example.option",
                        "operator": operator,
                        "value_type": value_type,
                        "value": value
                    }
                }]
            });
            let parsed = validate_policy_type_config(COMPOSITE_POLICY_TYPE, &config)
                .unwrap()
                .unwrap();
            assert_eq!(serde_json::to_value(parsed).unwrap(), config);
        }
    }

    #[test]
    fn composite_aggregation_never_promotes_error_or_not_checked_to_pass() {
        let result = |outcome| CompositeRuleOutcome {
            rule_id: Uuid::new_v4(),
            kind: "custom_eval".to_string(),
            phase: EnforcementPhase::Evaluation,
            outcome,
            blocking: outcome != EnforcementOutcome::Pass,
            detail: String::new(),
            evidence: serde_json::json!({}),
        };
        assert_eq!(
            aggregate_composite_outcomes(&[result(EnforcementOutcome::Pass)]),
            EnforcementOutcome::Pass
        );
        assert_eq!(
            aggregate_composite_outcomes(&[
                result(EnforcementOutcome::Pass),
                result(EnforcementOutcome::NotChecked),
            ]),
            EnforcementOutcome::NotChecked
        );
        assert_eq!(
            aggregate_composite_outcomes(&[
                result(EnforcementOutcome::Fail),
                result(EnforcementOutcome::Error),
            ]),
            EnforcementOutcome::Error
        );
    }

    #[test]
    fn composite_nix_literals_preserve_interpolation_backslashes_quotes_and_lines() {
        let policy_id = Uuid::from_u128(9);
        let config = CompositePolicyConfig {
            schema_version: 1,
            mode: CompositeRuleMode::All,
            rules: vec![CompositeRule {
                id: Uuid::from_u128(10),
                rule: CompositeRuleKind::NixosOption(NixosOptionRuleConfig {
                    path: "services.example.text".to_string(),
                    operator: "==".to_string(),
                    value_type: NixosOptionValueType::Lines,
                    value: serde_json::json!("literal ${value} \\\"quoted\\\"\nnext"),
                }),
            }],
        };
        let fields = build_policy_fields_for_config_standalone(&[AssignedPolicy {
            policy_id,
            policy_name: "semantic literals".to_string(),
            policy: DeploymentPolicy::Composite { config },
        }]);
        let expression = fields.join("\n");
        assert!(expression.contains("builtins.foldl'"));
        assert!(expression.contains("\\${value}"));
        assert!(expression.contains("\\\\\\\"quoted\\\\\\\""));
        assert!(expression.contains("\\nnext"));
        assert!(expression.contains("builtins.tryEval"));
    }

    #[test]
    fn legacy_empty_custom_check_remains_outside_composite_validation() {
        assert_eq!(
            validate_policy_type_config(
                "custom_check",
                &serde_json::json!({"mode": "all", "rules": []})
            )
            .unwrap(),
            None
        );
    }

    #[test]
    fn composite_missing_evaluator_evidence_is_error_and_reporting_is_not_checked() {
        let config = validate_policy_type_config(COMPOSITE_POLICY_TYPE, &composite_config())
            .unwrap()
            .unwrap();
        let assigned = AssignedPolicy {
            policy_id: Uuid::from_u128(99),
            policy_name: "composite".into(),
            policy: DeploymentPolicy::Composite { config },
        };
        let evaluated = PolicyCheckResult::from_assigned(
            "host".into(),
            &serde_json::json!({"cfAgentEnabled": true}),
            std::slice::from_ref(&assigned),
        )
        .unwrap();
        assert!(!evaluated.meets_requirements);
        let result = evaluated.assigned_results.get(&assigned.policy_id).unwrap();
        assert_eq!(result.passed, Some(false));
        assert!(
            result
                .composite_outcomes
                .iter()
                .any(|outcome| outcome.outcome == EnforcementOutcome::Error)
        );

        let check = PolicyCheckResult {
            system_name: "host".into(),
            cf_agent_enabled: Some(true),
            assigned_results: BTreeMap::new(),
            has_required_packages: None,
            custom_checks: HashMap::new(),
            meets_requirements: true,
            warnings: Vec::new(),
            failed_policies: Vec::new(),
            cve_checks: Vec::new(),
        };
        assert_eq!(
            policy_results_json(&check, &[assigned])["assigned"][Uuid::from_u128(99).to_string()]["passed"],
            serde_json::Value::Null
        );
    }

    #[test]
    fn composite_parser_preserves_pass_fail_error_and_pin_evidence() {
        let policy_id = Uuid::from_u128(200);
        let pass_id = Uuid::from_u128(201);
        let fail_id = Uuid::from_u128(202);
        let error_id = Uuid::from_u128(203);
        let pin_id = Uuid::from_u128(204);
        let config = CompositePolicyConfig {
            schema_version: 1,
            mode: CompositeRuleMode::All,
            rules: vec![
                CompositeRule {
                    id: pass_id,
                    rule: CompositeRuleKind::CustomEval(CustomEvalRuleConfig {
                        expression: "true".to_string(),
                        message: "pass".to_string(),
                    }),
                },
                CompositeRule {
                    id: fail_id,
                    rule: CompositeRuleKind::CustomEval(CustomEvalRuleConfig {
                        expression: "false".to_string(),
                        message: "fail".to_string(),
                    }),
                },
                CompositeRule {
                    id: error_id,
                    rule: CompositeRuleKind::CustomEval(CustomEvalRuleConfig {
                        expression: "throw \"contained\"".to_string(),
                        message: "error".to_string(),
                    }),
                },
                CompositeRule {
                    id: pin_id,
                    rule: CompositeRuleKind::PinRequired(EmptyRuleConfig {}),
                },
            ],
        };
        let assigned = AssignedPolicy {
            policy_id,
            policy_name: "mixed outcomes".to_string(),
            policy: DeploymentPolicy::Composite { config },
        };
        let metadata = serde_json::json!({
            "cfAgentEnabled": true,
            "requestedSourceRevision": "0123456789abcdef0123456789abcdef01234567",
            "resolvedSourceRevision": "0123456789abcdef0123456789abcdef01234567",
            composite_rule_result_key(&policy_id, &pass_id): {"success": true, "value": true},
            composite_rule_result_key(&policy_id, &fail_id): {"success": true, "value": false},
            composite_rule_result_key(&policy_id, &error_id): {"success": false, "value": false}
        });
        let check =
            PolicyCheckResult::from_assigned("host".to_string(), &metadata, &[assigned]).unwrap();
        let outcomes = &check.assigned_results[&policy_id].composite_outcomes;
        assert_eq!(outcomes[0].outcome, EnforcementOutcome::Pass);
        assert_eq!(outcomes[1].outcome, EnforcementOutcome::Fail);
        assert_eq!(outcomes[2].outcome, EnforcementOutcome::Error);
        assert_eq!(outcomes[3].outcome, EnforcementOutcome::Pass);
        assert_eq!(
            outcomes[3].evidence["resolved_revision"],
            "0123456789abcdef0123456789abcdef01234567"
        );
    }

    #[test]
    fn pin_required_requires_exact_full_immutable_requested_revision() {
        let policy_id = Uuid::from_u128(210);
        let pin_id = Uuid::from_u128(211);
        let assigned = AssignedPolicy {
            policy_id,
            policy_name: "pin".to_string(),
            policy: DeploymentPolicy::Composite {
                config: CompositePolicyConfig {
                    schema_version: 1,
                    mode: CompositeRuleMode::All,
                    rules: vec![CompositeRule {
                        id: pin_id,
                        rule: CompositeRuleKind::PinRequired(EmptyRuleConfig {}),
                    }],
                },
            },
        };
        let revision = "0123456789abcdef0123456789abcdef01234567";
        let mismatch = "1123456789abcdef0123456789abcdef01234567";
        for (expected, resolved, outcome) in [
            (Some(revision), Some(revision), EnforcementOutcome::Pass),
            (Some(revision), Some(mismatch), EnforcementOutcome::Fail),
            (Some(revision), None, EnforcementOutcome::Fail),
            (Some("main"), Some(revision), EnforcementOutcome::Error),
            (None, Some(revision), EnforcementOutcome::Error),
        ] {
            let metadata = serde_json::json!({
                "cfAgentEnabled": true,
                "requestedSourceRevision": expected,
                "resolvedSourceRevision": resolved,
            });
            let check = PolicyCheckResult::from_assigned(
                "host".to_string(),
                &metadata,
                std::slice::from_ref(&assigned),
            )
            .unwrap();
            let result = &check.assigned_results[&policy_id].composite_outcomes[0];
            assert_eq!(result.outcome, outcome, "{metadata}");
            assert_eq!(result.phase, EnforcementPhase::Evaluation);
            assert_eq!(result.blocking, outcome != EnforcementOutcome::Pass);
            assert_eq!(
                result.evidence["expected_revision"],
                serde_json::json!(expected)
            );
            assert_eq!(
                result.evidence["resolved_revision"],
                serde_json::json!(resolved)
            );
        }
    }

    #[test]
    fn eval_passed_uses_terminal_outcome_and_preserves_policy_version_identity() {
        let policy_id = Uuid::from_u128(220);
        let rule_id = Uuid::from_u128(221);
        let assigned = AssignedPolicy {
            policy_id,
            policy_name: "evaluation terminal".to_string(),
            policy: DeploymentPolicy::Composite {
                config: CompositePolicyConfig {
                    schema_version: 1,
                    mode: CompositeRuleMode::All,
                    rules: vec![CompositeRule {
                        id: rule_id,
                        rule: CompositeRuleKind::EvalPassed(EmptyRuleConfig {}),
                    }],
                },
            },
        };
        let passed = PolicyCheckResult::from_assigned(
            "host".to_string(),
            &serde_json::json!({"cfAgentEnabled": true}),
            std::slice::from_ref(&assigned),
        )
        .expect("successful evaluator metadata must produce eval_passed evidence");
        let passed = &passed.assigned_results[&policy_id].composite_outcomes[0];
        assert_eq!(passed.rule_id, rule_id);
        assert_eq!(passed.phase, EnforcementPhase::Evaluation);
        assert_eq!(passed.outcome, EnforcementOutcome::Pass);
        assert!(!passed.blocking);
        assert_eq!(passed.evidence["configuration"], "host");

        for (terminal, expected) in [
            (
                EvaluationTerminalOutcome::ConfirmedFailure,
                EnforcementOutcome::Fail,
            ),
            (EvaluationTerminalOutcome::Error, EnforcementOutcome::Error),
            (
                EvaluationTerminalOutcome::Pending,
                EnforcementOutcome::NotChecked,
            ),
        ] {
            let check = PolicyCheckResult::for_evaluation_terminal(
                "host".to_string(),
                std::slice::from_ref(&assigned),
                terminal,
                "terminal detail",
            );
            let result = check
                .assigned_results
                .get(&policy_id)
                .expect("exact policy version ID must be retained");
            assert_eq!(result.composite_outcomes[0].rule_id, rule_id);
            assert_eq!(result.composite_outcomes[0].outcome, expected);
            assert_eq!(
                result.composite_outcomes[0].phase,
                EnforcementPhase::Evaluation
            );
            assert!(result.composite_outcomes[0].blocking);
            assert!(
                result.composite_outcomes[0].evidence["terminal_outcome"]
                    .as_str()
                    .is_some()
            );
        }
    }

    /// Test helper: wrap a flat `Vec<DeploymentPolicy>` into a `PoliciesByConfiguration`
    /// under the key `"test-config"` with sequential UUIDs.
    fn policies_map_for(policies: Vec<DeploymentPolicy>) -> PoliciesByConfiguration {
        let mut map = PoliciesByConfiguration::new();
        let assigned: Vec<AssignedPolicy> = policies
            .into_iter()
            .enumerate()
            .map(|(i, policy)| AssignedPolicy {
                policy_id: uuid::Uuid::from_u128(i as u128 + 1),
                policy_name: format!("test-policy-{i}"),
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
        let (field_name, expr) = policy.to_nix_expression().unwrap();
        assert_eq!(field_name, "cfAgentEnabled");
        assert!(expr.contains("systemd.services.crystal-forge-agent.enable"));
        assert!(expr.contains("services.crystal-forge.enable"));
        assert!(expr.contains("services.crystal-forge.client.enable"));
    }

    #[test]
    fn crystal_forge_agent_policy_expression_checks_systemd_service() {
        let policy = DeploymentPolicy::RequireCrystalForgeAgent { strict: true };
        let (_, expr) = policy.to_nix_expression().unwrap();

        assert!(
            expr.starts_with("(config.systemd.services.crystal-forge-agent.enable or false)"),
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
        let (field_name, expr) = policy.to_nix_expression_with_index(2).unwrap();
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
        assert!(expr.contains("cfg.config.system.build.toplevel"));

        for forbidden in [
            "cfg.options",
            "configuration.options",
            "_module.graph",
            "lib.evalModules",
            "carrierConfigurationSources",
            "carrierModuleSnapshot",
            "safeCarrierModuleSnapshot",
            "flake.nixosModules",
            "evaluationSnapshot",
            "flakeOutputSnapshot",
            "__crystalForgeFlakeOutput",
        ] {
            assert!(
                !expr.contains(forbidden),
                "primary evaluator must not contain {forbidden}"
            );
        }
    }

    #[test]
    fn generated_evaluation_expression_parses_when_nix_is_available() {
        // Nix package builds cannot run nested Nix commands because the build
        // sandbox has no writable Nix state or daemon. The structural tests
        // above still validate the generated expression in that environment.
        if std::env::var_os("NIX_BUILD_TOP").is_some() {
            return;
        }
        let expr = build_nix_eval_expression(
            "git+https://example.test/flake.git?rev=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            &PoliciesByConfiguration::new(),
        );
        let output = match std::process::Command::new("nix-instantiate")
            .args(["--parse", "--expr", &expr])
            .output()
        {
            Ok(output) => output,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
            Err(error) => panic!("failed to run nix-instantiate: {error}"),
        };
        assert!(
            output.status.success(),
            "generated Nix expression did not parse: {}",
            String::from_utf8_lossy(&output.stderr)
        );
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
                    policy_name: "require-grafana".to_string(),
                    policy: DeploymentPolicy::RequirePackages {
                        packages: vec!["grafana".to_string()],
                        strict: true,
                    },
                },
                AssignedPolicy {
                    policy_id: id2,
                    policy_name: "require-neovim".to_string(),
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
                    policy_name: "require-cf-agent".to_string(),
                    policy: DeploymentPolicy::RequireCrystalForgeAgent { strict: true },
                },
                AssignedPolicy {
                    policy_id: id2,
                    policy_name: "require-cve-check".to_string(),
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
                policy_name: "require-grafana".to_string(),
                policy: DeploymentPolicy::RequirePackages {
                    packages: vec!["grafana".to_string()],
                    strict: true,
                },
            }],
        );

        let expr = build_nix_eval_expression("github:user/repo", &map);

        // Checker receives the canonical config object as `config`.
        assert!(
            expr.contains("\"gray\" = config:"),
            "bulk checker must bind canonical config, got:\n{expr}"
        );
        // The checker receives the full cfg object while policy expressions use
        // the canonical public `config` binding.
        assert!(
            expr.contains("config.environment.systemPackages"),
            "package policy must use config scope, got:\n{expr}"
        );
        // The checker is invoked with the public `config` binding from the
        // full cfg object.
        assert!(
            expr.contains("checker cfg.config"),
            "checker must receive cfg.config as its public config binding, got:\n{expr}"
        );
        assert!(!expr.contains("\"gray\" = cfg:"));
    }

    #[test]
    fn standalone_agent_policy_uses_full_cfg_object() {
        let assigned = vec![AssignedPolicy {
            policy_id: uuid::Uuid::from_u128(1),
            policy_name: "require-cf-agent".to_string(),
            policy: DeploymentPolicy::RequireCrystalForgeAgent { strict: true },
        }];

        let expr = crate::models::evaluate_with_policies::build_single_system_eval_expression(
            "github:user/repo",
            "gray",
            &assigned,
        );

        // Both the unconditional cfAgentEnabled and the assigned-policy stable
        // key must use the canonical config binding.
        assert!(
            expr.contains("config.systemd.services.crystal-forge-agent.enable"),
            "standalone agent check must use config scope, got:\n{expr}"
        );
        assert!(!expr.contains("cfg.config.systemd.services"));
    }

    #[test]
    fn bulk_custom_check_uses_canonical_config_scope() {
        let mut map = PoliciesByConfiguration::new();
        map.insert(
            "gray".to_string(),
            vec![AssignedPolicy {
                policy_id: uuid::Uuid::from_u128(1),
                policy_name: "firewall-enabled".to_string(),
                policy: DeploymentPolicy::CustomCheck {
                    expression: "config.networking.firewall.enable".to_string(),
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
            expr.contains("\"gray\" = config:"),
            "bulk checker must bind canonical config, got:\n{expr}"
        );
        assert!(
            expr.contains("config.networking.firewall.enable"),
            "custom check expression must use canonical config scope, got:\n{expr}"
        );
    }

    #[test]
    fn bulk_multi_rule_custom_check_preserves_cfg_scope() {
        let mut map = PoliciesByConfiguration::new();
        map.insert(
            "gray".to_string(),
            vec![AssignedPolicy {
                policy_id: uuid::Uuid::from_u128(1),
                policy_name: "ssh-and-firewall".to_string(),
                policy: DeploymentPolicy::CustomCheck {
                    expression: String::new(),
                    description: "ssh-and-firewall".to_string(),
                    field_name: "parent".to_string(),
                    strict: true,
                    rules: vec![
                        PolicyRule {
                            expression: "config.services.openssh.enable".to_string(),
                            description: "ssh".to_string(),
                            field_name: "sshEnabled".to_string(),
                            strict: true,
                        },
                        PolicyRule {
                            expression: "config.networking.firewall.enable".to_string(),
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
            expr.contains("\"gray\" = config:"),
            "bulk checker must bind canonical config, got:\n{expr}"
        );
        assert!(
            expr.contains("config.services.openssh.enable"),
            "multi-rule expression must be emitted with config scope, got:\n{expr}"
        );
        assert!(
            expr.contains("config.networking.firewall.enable"),
            "multi-rule expression must be emitted with config scope, got:\n{expr}"
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
                policy_name: "require-cf-agent".to_string(),
                policy: DeploymentPolicy::RequireCrystalForgeAgent { strict: true },
            },
            AssignedPolicy {
                policy_id: uuid::Uuid::from_u128(2),
                policy_name: "require-grafana".to_string(),
                policy: DeploymentPolicy::RequirePackages {
                    packages: vec!["grafana".to_string()],
                    strict: true,
                },
            },
            AssignedPolicy {
                policy_id: uuid::Uuid::from_u128(3),
                policy_name: "ssh-and-firewall".to_string(),
                policy: DeploymentPolicy::CustomCheck {
                    expression: String::new(),
                    description: "ssh-and-firewall".to_string(),
                    field_name: "parent".to_string(),
                    strict: true,
                    rules: vec![
                        PolicyRule {
                            expression: "config.services.openssh.enable".to_string(),
                            description: "ssh".to_string(),
                            field_name: "sshEnabled".to_string(),
                            strict: true,
                        },
                        PolicyRule {
                            expression: "config.networking.firewall.enable".to_string(),
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
    #[ignore = "requires Nix evaluator in PATH"]
    fn required_packages_expression_evaluates_each_requested_package() {
        let (_, expression) = DeploymentPolicy::RequirePackages {
            packages: vec!["openssh".into(), "auditd".into(), "aide".into()],
            strict: true,
        }
        .to_nix_expression_with_index(0)
        .unwrap();

        for (installed, expected) in [
            (vec!["openssh", "auditd", "aide"], true),
            (vec!["openssh", "auditd"], false),
            (Vec::new(), false),
        ] {
            let packages = installed
                .iter()
                .map(|name| format!("{{ pname = \"{name}\"; name = \"{name}\"; }}"))
                .collect::<Vec<_>>()
                .join(" ");
            let nix_expression =
                format!("let config.environment.systemPackages = [ {packages} ]; in {expression}");
            let output = std::process::Command::new("nix")
                .args(["eval", "--json", "--expr", &nix_expression])
                .output()
                .expect("failed to spawn nix eval");
            assert!(
                output.status.success(),
                "nix eval failed:\n{}\nExpression:\n{}",
                String::from_utf8_lossy(&output.stderr),
                nix_expression
            );
            let actual: bool = serde_json::from_slice(&output.stdout).expect("boolean JSON result");
            assert_eq!(actual, expected, "installed packages: {installed:?}");
        }
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
            policy_name: "require-grafana".to_string(),
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
            policy_name: "firewall-enabled".to_string(),
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
            policy_name: "spoof-agent".to_string(),
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
            policy_name: "mixed-rules".to_string(),
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
            policy_name: "failme".to_string(),
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
        // The persisted "name" must be the real DB policy name (used for the
        // matrix column header and "View policy definition" navigation),
        // not the generated description string.
        assert_eq!(
            result.get("name").and_then(|value| value.as_str()),
            Some("failme")
        );
        assert_eq!(
            result.get("description").and_then(|value| value.as_str()),
            Some("Required packages: grafana")
        );
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

    // ── Global CF-agent invariant regression ────────────────────────────
    //
    // Scenario from review: a configuration with NO assigned
    // require_cf_agent policy but cfAgentEnabled=false must never report
    // meets_requirements=true. Without this invariant the live evaluator
    // could log "evaluation and policies passed" for a system that the
    // database gate (policy_requirements_met) was simultaneously blocking
    // from being queued — a contradictory user-facing result.
    #[test]
    fn agent_disabled_with_no_assigned_policies_fails_meets_requirements() {
        let check = PolicyCheckResult::from_assigned(
            "gray".to_string(),
            &serde_json::json!({ "cfAgentEnabled": false }),
            &[],
        )
        .expect("policy metadata should parse");

        assert_eq!(check.cf_agent_enabled, Some(false));
        assert!(
            !check.meets_requirements,
            "meets_requirements must be false when cfAgentEnabled=false, \
             even with zero assigned policies"
        );
        assert!(
            check.failed_policies.iter().any(|(_, strict)| *strict),
            "a strict failure must be recorded so system_not_queued_reason \
             and the caller-visible status agree with policy_requirements_met"
        );
        assert!(!policy_requirements_met(&check));
    }

    #[test]
    fn agent_disabled_failure_is_not_duplicated_when_require_cf_agent_is_assigned() {
        let policy_id = uuid::Uuid::from_u128(1);
        let assigned = vec![AssignedPolicy {
            policy_id,
            policy_name: "require-cf-agent".to_string(),
            policy: DeploymentPolicy::RequireCrystalForgeAgent { strict: true },
        }];
        let check = PolicyCheckResult::from_assigned(
            "gray".to_string(),
            &serde_json::json!({
                "cfAgentEnabled": false,
                policy_result_key(&policy_id): false,
            }),
            &assigned,
        )
        .expect("policy metadata should parse");

        assert!(!check.meets_requirements);
        assert_eq!(
            check.failed_policies.len(),
            1,
            "the assigned require_cf_agent failure and the global invariant \
             must not both contribute a failed_policies entry: {:?}",
            check.failed_policies
        );
    }

    // ── Multiple policies of the same type persist distinct results ─────
    //
    // Regression for the P2 finding: `has_required_packages` is a single
    // shared field, so before `assigned_results` was keyed by UUID, two
    // `RequirePackages` policies assigned to the same configuration could
    // collapse to a single boolean and both appear to pass/fail together
    // depending on iteration order.
    #[test]
    fn two_require_packages_policies_persist_distinct_results() {
        let policy_a = uuid::Uuid::from_u128(0xA);
        let policy_b = uuid::Uuid::from_u128(0xB);
        let assigned = vec![
            AssignedPolicy {
                policy_id: policy_a,
                policy_name: "require-grafana".to_string(),
                policy: DeploymentPolicy::RequirePackages {
                    packages: vec!["grafana".to_string()],
                    strict: true,
                },
            },
            AssignedPolicy {
                policy_id: policy_b,
                policy_name: "require-neovim".to_string(),
                policy: DeploymentPolicy::RequirePackages {
                    packages: vec!["neovim".to_string()],
                    strict: true,
                },
            },
        ];

        let check = PolicyCheckResult::from_assigned(
            "gray".to_string(),
            &serde_json::json!({
                "cfAgentEnabled": true,
                policy_result_key(&policy_a): false,
                policy_result_key(&policy_b): true,
            }),
            &assigned,
        )
        .expect("policy metadata should parse");

        // Per-UUID map must retain the distinct per-policy outcomes.
        assert_eq!(
            check.assigned_results.get(&policy_a).and_then(|r| r.passed),
            Some(false)
        );
        assert_eq!(
            check.assigned_results.get(&policy_b).and_then(|r| r.passed),
            Some(true)
        );

        // The persisted JSON (the new UI source of truth) must not collapse
        // the two policies to the same outcome.
        let persisted = policy_results_json(&check, &assigned);
        let assigned_json = persisted
            .get("assigned")
            .and_then(|v| v.as_object())
            .expect("assigned map should be present");
        let passed_a = assigned_json
            .get(&policy_a.to_string())
            .and_then(|v| v.get("passed"))
            .and_then(|v| v.as_bool());
        let passed_b = assigned_json
            .get(&policy_b.to_string())
            .and_then(|v| v.get("passed"))
            .and_then(|v| v.as_bool());
        assert_eq!(passed_a, Some(false), "policy A must persist as failed");
        assert_eq!(passed_b, Some(true), "policy B must persist as passed");
        assert_ne!(
            passed_a, passed_b,
            "two RequirePackages policies with different outcomes must not \
             collapse to the same persisted result"
        );

        // Queue gate must reflect the strict failure from policy A even
        // though policy B independently passed.
        assert!(!policy_requirements_met(&check));
    }

    // ── CF-agent mismatch hardening ───────────────────────────────────────
    //
    // The assigned require_cf_agent policy's stable-key result and the
    // unconditional cfAgentEnabled metadata are generated from the same
    // underlying Nix expression and must always agree. A future generator
    // regression that lets them diverge must be treated as an
    // infrastructure error, not silently resolved by letting one value
    // overwrite the other.
    #[test]
    fn assigned_cf_agent_result_disagreeing_with_unconditional_is_infrastructure_error() {
        let policy_id = uuid::Uuid::from_u128(1);
        let assigned = vec![AssignedPolicy {
            policy_id,
            policy_name: "require-cf-agent".to_string(),
            policy: DeploymentPolicy::RequireCrystalForgeAgent { strict: true },
        }];
        let policies_json = serde_json::json!({
            "cfAgentEnabled": true,
            policy_result_key(&policy_id): false,
        });

        let result =
            PolicyCheckResult::from_assigned("gray".to_string(), &policies_json, &assigned);

        assert!(
            result.is_err(),
            "disagreement between unconditional and assigned CF-agent results \
             must be treated as an infrastructure error"
        );
        let err = result.unwrap_err();
        assert!(
            err.contains("disagrees"),
            "error should describe the disagreement: {err}"
        );
    }

    #[test]
    fn assigned_cf_agent_result_agreeing_with_unconditional_parses_normally() {
        let policy_id = uuid::Uuid::from_u128(1);
        let assigned = vec![AssignedPolicy {
            policy_id,
            policy_name: "require-cf-agent".to_string(),
            policy: DeploymentPolicy::RequireCrystalForgeAgent { strict: true },
        }];
        let policies_json = serde_json::json!({
            "cfAgentEnabled": true,
            policy_result_key(&policy_id): true,
        });

        let check = PolicyCheckResult::from_assigned("gray".to_string(), &policies_json, &assigned)
            .expect("agreeing values should parse normally");

        assert_eq!(check.cf_agent_enabled, Some(true));
        assert!(check.meets_requirements);
    }

    // ── Legacy require_cf_agent deduplication ────────────────────────────

    #[test]
    fn no_assigned_policies_agent_enabled_passes() {
        // When no policies are assigned and the global cfAgentEnabled
        // metadata is true, the check must pass with zero assigned
        // results (the caller filters out require_cf_agent before
        // calling from_assigned, so the assigned list is empty).
        let check = PolicyCheckResult::from_assigned(
            "gray".to_string(),
            &serde_json::json!({ "cfAgentEnabled": true }),
            &[],
        )
        .expect("should parse with only global cfAgentEnabled");

        assert_eq!(check.cf_agent_enabled, Some(true));
        assert!(check.meets_requirements);
        assert!(check.assigned_results.is_empty());
    }

    #[test]
    fn no_assigned_policies_agent_disabled_fails_strictly() {
        // The caller filters out require_cf_agent, so the assigned list
        // is empty. Global cfAgentEnabled=false must still produce a
        // strict failure — the global invariant is independent of
        // whether a legacy require_cf_agent policy was assigned.
        let check = PolicyCheckResult::from_assigned(
            "gray".to_string(),
            &serde_json::json!({ "cfAgentEnabled": false }),
            &[],
        )
        .expect("should parse when agent disabled with empty assigned");

        assert_eq!(check.cf_agent_enabled, Some(false));
        assert!(!check.meets_requirements);
        assert!(
            check.failed_policies.iter().any(|(_, strict)| *strict),
            "global cfAgentEnabled=false must produce a strict failure \
             even when the caller filtered out require_cf_agent assignments"
        );
    }

    #[test]
    fn contradictory_legacy_cf_agent_assignment_is_infrastructure_error() {
        let policy_id = uuid::Uuid::from_u128(0xDEAD);
        let assigned = vec![AssignedPolicy {
            policy_id,
            policy_name: "Require Crystal Forge Agent".to_string(),
            policy: DeploymentPolicy::RequireCrystalForgeAgent { strict: false },
        }];

        // Global says agent disabled BUT legacy result key says true.
        // This should be caught by the mismatch guard and be treated
        // as an infrastructure error — the assigned value must not
        // override the unconditional global metadata.
        let result = PolicyCheckResult::from_assigned(
            "gray".to_string(),
            &serde_json::json!({
                "cfAgentEnabled": false,
                policy_result_key(&policy_id): true,
            }),
            &assigned,
        );

        assert!(
            result.is_err(),
            "disagreement between global cfAgentEnabled (false) and \
             legacy assigned key (true) must be an infrastructure error"
        );
        let err = result.unwrap_err();
        assert!(
            err.contains("disagrees"),
            "error should describe the disagreement: {err}"
        );
    }
}
