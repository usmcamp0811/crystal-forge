//! Shared types for policy components.

use uuid::Uuid;

/// Policy definition format.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PolicyFormat {
    Toml,
    Json,
}

/// A deployment policy definition.
#[derive(Clone, Debug, PartialEq)]
pub struct PolicyDefinition {
    pub id: Uuid,
    /// Real policy lineage identifier. `id` is retained as a compatibility alias.
    pub lineage_id: Uuid,
    /// Exact version used for interchange export, if supplied by the API.
    pub version_id: Option<Uuid>,
    pub revision: Option<String>,
    pub publication_state: Option<String>,
    pub semantic_digest: Option<String>,
    pub revisions: Vec<PolicyRevisionSummary>,
    pub name: String,
    pub description: String,
    pub format: PolicyFormat,
    pub body: String,
    /// The policy type (e.g., "require_cf_agent", "require_crystal_forge_agent", "require_packages", "custom_check").
    /// Optional for backward compatibility with mock/TOML policies that don't have this field.
    pub policy_type: Option<String>,
    /// Number of NixOS derivations (systems) this policy applies to.
    pub system_count: i64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PolicyRevisionSummary {
    pub id: Uuid,
    pub version: String,
    pub publication_state: String,
    pub trust_state: String,
    pub semantic_digest: String,
    pub created_at: String,
    pub is_current_published: bool,
    pub is_current_draft: bool,
    pub name: String,
    pub description: Option<String>,
    pub policy_type: String,
    pub config: serde_json::Value,
    pub enabled: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PolicyCategory {
    Deployment,
    Security,
    Scanning,
    Rollout,
}

impl PolicyCategory {
    pub fn id(self) -> &'static str {
        match self {
            Self::Deployment => "deployment",
            Self::Security => "security",
            Self::Scanning => "scanning",
            Self::Rollout => "rollout",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Deployment => "Deployment gates",
            Self::Security => "Security baseline",
            Self::Scanning => "Vulnerability gates",
            Self::Rollout => "Rollout controls",
        }
    }

    pub fn short_label(self) -> &'static str {
        match self {
            Self::Deployment => "deploy",
            Self::Security => "secure",
            Self::Scanning => "scan",
            Self::Rollout => "rollout",
        }
    }

    pub fn blurb(self) -> &'static str {
        match self {
            Self::Deployment => "Criteria a system must satisfy before deploy.",
            Self::Security => "Host configuration and hardening assertions.",
            Self::Scanning => "CVE and scan-result blockers.",
            Self::Rollout => "Approval, timing, and canary constraints.",
        }
    }

    pub fn color(self) -> &'static str {
        match self {
            Self::Deployment => "#a78bfa",
            Self::Security => "#60a5fa",
            Self::Scanning => "#fbbf24",
            Self::Rollout => "#34d399",
        }
    }
}

pub const POLICY_CATEGORIES: [PolicyCategory; 4] = [
    PolicyCategory::Deployment,
    PolicyCategory::Security,
    PolicyCategory::Scanning,
    PolicyCategory::Rollout,
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PolicyRuleSummary {
    pub label: String,
}

pub fn is_core_policy(policy: &PolicyDefinition) -> bool {
    policy.policy_type.as_ref().map_or(false, |policy_type| {
        policy_type == "require_cf_agent" || policy_type == "require_crystal_forge_agent"
    })
}

pub fn is_policy_enabled(policy: &PolicyDefinition) -> bool {
    policy_config(policy)
        .and_then(|config| config.get("enabled").and_then(|value| value.as_bool()))
        .unwrap_or(true)
}

pub fn policy_category(policy: &PolicyDefinition) -> PolicyCategory {
    let policy_type = normalized_policy_type(policy);
    let config = policy_config(policy).unwrap_or(serde_json::Value::Null);

    if policy_type == "require_cve_check" || config.get("max_critical").is_some() {
        return PolicyCategory::Scanning;
    }

    if let Some(rules) = config.get("rules").and_then(|value| value.as_array()) {
        if rules.iter().any(|rule| {
            let kind = rule
                .get("kind")
                .and_then(|value| value.as_str())
                .unwrap_or_default();
            matches!(
                kind,
                "time_window" | "approval_required" | "rollout_percent"
            )
        }) {
            return PolicyCategory::Rollout;
        }

        if rules.iter().any(|rule| {
            let kind = rule
                .get("kind")
                .and_then(|value| value.as_str())
                .unwrap_or_default();
            matches!(kind, "packages_installed" | "nixos_option" | "custom_eval")
        }) {
            return PolicyCategory::Security;
        }
    }

    if policy_type == "require_packages"
        || policy_type == "custom_check"
        || policy_type == "require_cf_agent"
        || policy_type == "require_crystal_forge_agent"
    {
        PolicyCategory::Security
    } else {
        PolicyCategory::Deployment
    }
}

pub fn policy_rule_summaries(policy: &PolicyDefinition) -> Vec<PolicyRuleSummary> {
    let policy_type = normalized_policy_type(policy);
    let config = policy_config(policy).unwrap_or(serde_json::Value::Null);

    if let Some(rules) = config.get("rules").and_then(|value| value.as_array()) {
        let summaries = rules
            .iter()
            .map(rule_summary_from_json)
            .filter(|summary| !summary.label.is_empty())
            .collect::<Vec<_>>();
        if !summaries.is_empty() {
            return summaries;
        }
    }

    match policy_type.as_str() {
        "require_cf_agent" | "require_crystal_forge_agent" => vec![PolicyRuleSummary {
            label: "Crystal Forge agent must be enabled".to_string(),
        }],
        "require_packages" => {
            let packages = config
                .get("packages")
                .and_then(|value| value.as_array())
                .map(|items| {
                    items
                        .iter()
                        .filter_map(|item| item.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                })
                .filter(|items| !items.is_empty())
                .unwrap_or_else(|| "required packages".to_string());
            vec![PolicyRuleSummary {
                label: format!("Packages present: {packages}"),
            }]
        }
        "require_cve_check" => vec![PolicyRuleSummary {
            label: cve_rule_label(&config),
        }],
        "custom_check" => {
            let label = config
                .get("description")
                .and_then(|value| value.as_str())
                .filter(|value| !value.trim().is_empty())
                .map(str::to_string)
                .or_else(|| {
                    config
                        .get("expression")
                        .and_then(|value| value.as_str())
                        .map(|expression| format!("Custom Nix assertion: {expression}"))
                })
                .unwrap_or_else(|| "Custom Nix assertion must pass".to_string());
            vec![PolicyRuleSummary { label }]
        }
        _ => vec![PolicyRuleSummary {
            label: format!("{} policy must pass", policy_type.replace('_', " ")),
        }],
    }
}

pub fn normalized_policy_type(policy: &PolicyDefinition) -> String {
    policy
        .policy_type
        .clone()
        .or_else(|| {
            policy_config(policy)
                .and_then(|config| {
                    config
                        .get("policy_type")
                        .or_else(|| config.get("type"))
                        .cloned()
                })
                .and_then(|value| value.as_str().map(ToString::to_string))
        })
        .unwrap_or_else(|| "custom_check".to_string())
}

fn policy_config(policy: &PolicyDefinition) -> Option<serde_json::Value> {
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(&policy.body) {
        return value.get("config").cloned().or(Some(value));
    }

    let mut in_policy_block = false;
    let mut map = serde_json::Map::new();

    for raw_line in policy.body.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line == "[[policy]]" {
            if in_policy_block && !map.is_empty() {
                break;
            }
            in_policy_block = true;
            continue;
        }
        if !in_policy_block {
            continue;
        }
        let Some((key, raw_value)) = line.split_once('=') else {
            continue;
        };
        map.insert(
            key.trim().to_string(),
            parse_tomlish_value(raw_value.trim()),
        );
    }

    if map.is_empty() {
        None
    } else {
        Some(serde_json::Value::Object(map))
    }
}

fn parse_tomlish_value(raw: &str) -> serde_json::Value {
    if raw.starts_with('"') && raw.ends_with('"') && raw.len() >= 2 {
        serde_json::Value::String(raw[1..raw.len() - 1].to_string())
    } else if raw == "true" || raw == "false" {
        serde_json::Value::Bool(raw == "true")
    } else if raw.starts_with('[') && raw.ends_with(']') {
        serde_json::Value::Array(
            raw[1..raw.len() - 1]
                .split(',')
                .map(str::trim)
                .filter(|item| !item.is_empty())
                .map(|item| serde_json::Value::String(item.trim_matches('"').to_string()))
                .collect(),
        )
    } else if let Ok(number) = raw.parse::<u64>() {
        serde_json::json!(number)
    } else {
        serde_json::Value::String(raw.to_string())
    }
}

fn rule_summary_from_json(rule: &serde_json::Value) -> PolicyRuleSummary {
    let kind = rule
        .get("kind")
        .and_then(|value| value.as_str())
        .unwrap_or_default();
    let label = match kind {
        "eval_passed" => "Evaluation must pass".to_string(),
        "build_succeeded" => "Build must succeed and be cacheable".to_string(),
        "cve_block" => {
            let severity = rule
                .get("severity")
                .and_then(|value| value.as_str())
                .unwrap_or("critical");
            let max = rule
                .get("maxAllowed")
                .and_then(|value| value.as_u64())
                .unwrap_or(0);
            format!("Block deploy when {severity} CVEs exceed {max}")
        }
        "time_window" => {
            let from = rule
                .get("from")
                .and_then(|value| value.as_str())
                .unwrap_or("09:00");
            let to = rule
                .get("to")
                .and_then(|value| value.as_str())
                .unwrap_or("17:00");
            format!("Deploy window: {from}-{to}")
        }
        "approval_required" => {
            let count = rule
                .get("count")
                .and_then(|value| value.as_u64())
                .unwrap_or(1);
            let role = rule
                .get("role")
                .and_then(|value| value.as_str())
                .unwrap_or("operator");
            format!("{count} {role} approval(s) required")
        }
        "rollout_percent" => {
            let percent = rule
                .get("percent")
                .and_then(|value| value.as_u64())
                .unwrap_or(25);
            format!("Canary rollout: {percent}% at a time")
        }
        "packages_installed" => {
            let packages = rule
                .get("packages")
                .and_then(|value| value.as_array())
                .map(|items| {
                    items
                        .iter()
                        .filter_map(|item| item.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                })
                .unwrap_or_default();
            format!("Packages installed: {packages}")
        }
        "nixos_option" => {
            let path = rule
                .get("path")
                .and_then(|value| value.as_str())
                .unwrap_or("option");
            let op = rule
                .get("op")
                .and_then(|value| value.as_str())
                .unwrap_or("==");
            let value = rule
                .get("value")
                .and_then(|value| value.as_str())
                .unwrap_or("expected");
            format!("config.{path} {op} {value}")
        }
        "custom_eval" => rule
            .get("message")
            .and_then(|value| value.as_str())
            .unwrap_or("Custom Nix expression must pass")
            .to_string(),
        _ => String::new(),
    };

    PolicyRuleSummary { label }
}

fn cve_rule_label(config: &serde_json::Value) -> String {
    let max_critical = config
        .get("max_critical")
        .and_then(|value| value.as_u64())
        .map(|value| format!("critical ≤ {value}"));
    let max_high = config
        .get("max_high")
        .and_then(|value| value.as_u64())
        .map(|value| format!("high ≤ {value}"));
    let when_no_scan = config
        .get("when_no_scan")
        .and_then(|value| value.as_str())
        .unwrap_or("block");

    [
        max_critical,
        max_high,
        Some(format!("no scan → {when_no_scan}")),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(" · ")
}

/// Sample TOML policy body used as a default in the editor.
pub const POLICY_TOML_SAMPLE: &str = r#"[[policy]]
type = "require_crystal_forge_agent"
strict = true

[[policy]]
type = "require_packages"
packages = ["git", "vim"]
strict = false

[[policy]]
type = "custom_check"
expression = "(cfg.config.services.openssh.enable or false)"
description = "SSH must be enabled"
field_name = "sshEnabled"
strict = true
"#;
