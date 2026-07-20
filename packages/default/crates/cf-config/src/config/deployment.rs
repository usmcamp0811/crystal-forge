use super::duration_serde;
use cf_protocol::cache::CacheType;
use ed25519_dalek::VerifyingKey;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;

// ─────────────────────────────────────────────────────────────────────────────
// Deployment policy types (pure serde; no SQLx)
// ─────────────────────────────────────────────────────────────────────────────

/// Behaviour when no CVE scan has been completed for the derivation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WhenNoScan {
    /// Treat missing scan as a policy violation.
    Block,
    /// Skip the check and allow deployment.
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
    #[serde(default)]
    pub max_critical: u32,
    #[serde(default)]
    pub max_high: Option<u32>,
    #[serde(default)]
    pub require_high_justification: bool,
    #[serde(default = "default_strict")]
    pub strict: bool,
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

/// Configuration for a `time_window` deployment policy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeWindowConfig {
    pub description: String,
    pub days: Vec<String>,
    pub start_time: String,
    pub end_time: String,
    pub timezone: String,
    #[serde(default = "default_action_block")]
    pub action: String,
}

fn default_action_block() -> String {
    "block".to_string()
}

/// Configuration for a `require_approvals` deployment policy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalConfig {
    pub description: String,
    pub count: u32,
    pub role: String,
    #[serde(default = "default_true")]
    pub distinct: bool,
    #[serde(default)]
    pub expires_after_hours: Option<u32>,
}

fn default_true() -> bool {
    true
}

/// Health check configuration for canary rollout.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckConfig {
    #[serde(rename = "type")]
    pub health_check_type: String,
    #[serde(default)]
    pub fail_threshold: u32,
}

/// Configuration for a `canary_rollout` deployment policy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanaryConfig {
    pub description: String,
    pub percentage: u32,
    pub observe_duration_minutes: u32,
    pub selection_strategy: String,
    pub health_check: HealthCheckConfig,
}

/// Action for a specific severity level.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SeverityAction {
    Block,
    Warn,
}

/// Threshold configuration for a specific severity level.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeverityThreshold {
    pub max: u32,
    pub action: SeverityAction,
}

/// Configuration for a `cve_threshold` deployment policy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CveThresholdConfig {
    pub description: String,
    pub thresholds: HashMap<String, SeverityThreshold>,
    #[serde(default = "default_no_scan_block")]
    pub no_scan_behavior: String,
    #[serde(default)]
    pub allow_justifications: bool,
    #[serde(default)]
    pub require_acknowledgment: bool,
}

fn default_no_scan_block() -> String {
    "block".to_string()
}

/// Mode for evaluating multiple rules.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleMode {
    All,
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
    pub expression: String,
    pub description: String,
    pub field_name: String,
    #[serde(default = "default_strict")]
    pub strict: bool,
}

/// A deployment policy that systems must satisfy.
///
/// Pure serde type — no SQLx, no DB. The server reads these from TOML config
/// and evaluates them during deployment gating.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DeploymentPolicy {
    RequireCrystalForgeAgent { strict: bool },
    RequirePackages { packages: Vec<String>, strict: bool },
    CustomCheck {
        #[serde(default)]
        expression: String,
        #[serde(default)]
        description: String,
        #[serde(default)]
        field_name: String,
        strict: bool,
        #[serde(default)]
        rules: Vec<PolicyRule>,
        #[serde(default)]
        mode: RuleMode,
    },
    RequireCveCheck { config: CveCheckConfig },
    TimeWindow { config: TimeWindowConfig },
    RequireApprovals { config: ApprovalConfig },
    CanaryRollout { config: CanaryConfig },
    CveThreshold { config: CveThresholdConfig },
}

impl DeploymentPolicy {
    pub fn is_strict(&self) -> bool {
        match self {
            DeploymentPolicy::RequireCrystalForgeAgent { strict }
            | DeploymentPolicy::RequirePackages { strict, .. }
            | DeploymentPolicy::CustomCheck { strict, .. } => *strict,
            DeploymentPolicy::RequireCveCheck { config } => config.strict,
            DeploymentPolicy::TimeWindow { config } => config.action == "block",
            DeploymentPolicy::RequireApprovals { .. } => true,
            DeploymentPolicy::CanaryRollout { .. } => true,
            DeploymentPolicy::CveThreshold { .. } => true,
        }
    }

    /// Returns true if this policy is evaluated via Nix.
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
}

// ─────────────────────────────────────────────────────────────────────────────
// DeploymentStrategy and DeploymentConfig
// ─────────────────────────────────────────────────────────────────────────────

/// Deployment strategy for activating configurations.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DeploymentStrategy {
    /// Create generation + activate immediately (default)
    ImmediatePersist,
    /// Create generation, activate on next boot only
    BootOnly,
}

impl Default for DeploymentStrategy {
    fn default() -> Self {
        Self::ImmediatePersist
    }
}

/// Configuration for deployment operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploymentConfig {
    #[serde(skip)]
    pub server_public_key: Option<VerifyingKey>,
    pub max_deployment_age_minutes: u64,
    pub dry_run_first: bool,
    pub fallback_to_local_build: bool,
    pub deployment_timeout_minutes: u64,
    pub cache_url: Option<String>,
    pub cache_public_key: Option<String>,
    #[serde(with = "duration_serde")]
    pub deployment_poll_interval: Duration,
    #[serde(
        default = "default_post_agent_start_deployment_delay",
        with = "duration_serde"
    )]
    pub post_agent_start_deployment_delay: Duration,

    /// Deployment policies that systems must satisfy.
    #[serde(default)]
    pub policies: Vec<DeploymentPolicy>,
    pub require_sigs: bool,

    /// Cache type (Attic, S3, Nix, Http)
    #[serde(default)]
    pub cache_type: CacheType,
    /// Attic cache name (used when cache_type is Attic)
    pub attic_cache_name: Option<String>,

    /// Deployment strategy (immediate_persist or boot_only)
    #[serde(default)]
    pub strategy: DeploymentStrategy,
}

fn default_post_agent_start_deployment_delay() -> Duration {
    Duration::from_secs(60)
}

impl Default for DeploymentConfig {
    fn default() -> Self {
        Self {
            server_public_key: None,
            max_deployment_age_minutes: 30,
            dry_run_first: true,
            fallback_to_local_build: false,
            deployment_timeout_minutes: 60,
            cache_url: None,
            cache_public_key: None,
            deployment_poll_interval: Duration::from_secs(60),
            post_agent_start_deployment_delay: default_post_agent_start_deployment_delay(),
            policies: vec![
                DeploymentPolicy::RequireCrystalForgeAgent { strict: false },
            ],
            require_sigs: true,
            cache_type: CacheType::Nix,
            attic_cache_name: None,
            strategy: DeploymentStrategy::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deployment_strategy_default() {
        let strategy = DeploymentStrategy::default();
        assert_eq!(strategy, DeploymentStrategy::ImmediatePersist);
    }

    #[test]
    fn test_deployment_config_default_strategy() {
        let config = DeploymentConfig::default();
        assert_eq!(config.strategy, DeploymentStrategy::ImmediatePersist);
    }

    #[test]
    fn post_agent_start_deployment_delay_defaults_when_missing() {
        let json = r#"{
            "max_deployment_age_minutes": 30,
            "dry_run_first": true,
            "fallback_to_local_build": false,
            "deployment_timeout_minutes": 60,
            "deployment_poll_interval": 900,
            "require_sigs": true
        }"#;

        let config: DeploymentConfig = serde_json::from_str(json).unwrap();
        assert_eq!(
            config.post_agent_start_deployment_delay,
            Duration::from_secs(60)
        );
    }
}
