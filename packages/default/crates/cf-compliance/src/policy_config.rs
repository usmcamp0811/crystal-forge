//! Defines the database-free half of deployment-policy configuration.
//!
//! This module holds the typed composite policy format, the structural and
//! syntax validation applied at immutable persistence and import boundaries,
//! and the small CVE-check configuration types that policy export shares with
//! the server. Evaluation, Nix expression generation, and persistence remain
//! in `cf-server`, which re-exports every public item here unchanged.
//!
//! Full validation ([`validate_policy_type_config`]) runs one bounded
//! `nix-instantiate --parse` subprocess for custom expressions. Offline tools
//! that must not start a subprocess use [`deserialize_policy_type_config`],
//! which performs every structural check and skips only the parser.

use serde::{Deserialize, Serialize};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
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
    crate::canonical::semantic_digest(&value)
}

/// Limits each composite custom expression to 16 KiB before syntax parsing.
pub const MAX_CUSTOM_EVAL_EXPRESSION_BYTES: usize = 16 * 1024;

/// Parses a dotted option path with optional JSON-quoted attribute segments.
///
/// # Errors
/// Returns an error for empty segments, malformed quoted strings, or bare
/// segments containing quotes, escapes, whitespace, or control characters.
pub fn parse_nixos_option_path(path: &str) -> Result<Vec<String>, String> {
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

pub(crate) fn validate_custom_eval_syntax(expressions: &[&str]) -> Result<(), String> {
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
    if policy_type == "custom_check" {
        crate::custom_check::validate_and_normalize_config(
            config,
            crate::custom_check::ExpressionBinding::Current,
            validate_nix_syntax,
        )?;
        return Ok(None);
    }
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
                crate::time_window::validate_window_parts(
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

/// Returns the default blocking policy for omitted `strict` fields.
pub fn default_strict() -> bool {
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
