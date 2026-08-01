use cf_protocol::builder::{RemoteBuildExecutionStrategy, SourceInputDeliveryMode};
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionMode {
    Real,
    Mock,
}

impl Default for ExecutionMode {
    fn default() -> Self {
        Self::Real
    }
}

impl ExecutionMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Real => "real",
            Self::Mock => "mock",
        }
    }

    pub fn is_mock(self) -> bool {
        matches!(self, Self::Mock)
    }
}

/// Configuration for the server itself.
///
/// This section is loaded from `[server]` in `config.toml`.
#[derive(Debug, Deserialize, Clone)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,

    /// Number of worker threads for nix-eval-jobs parallel evaluation.
    /// Default: 2 (conservative to avoid hosing the system)
    #[serde(default = "default_eval_workers")]
    pub eval_workers: usize,

    /// Maximum memory size per worker in MB for nix-eval-jobs.
    /// Total eval memory = eval_workers × eval_max_memory_mb
    /// Default: 4096 MB (4 GB) per worker
    #[serde(default = "default_eval_max_memory_mb")]
    pub eval_max_memory_mb: usize,

    /// Whether to check cache status during evaluation.
    /// Adds --check-cache-status flag to nix-eval-jobs.
    /// Default: true
    #[serde(default = "default_eval_check_cache")]
    pub eval_check_cache: bool,

    /// Automatically enqueue hardening scans after successful commit evaluation.
    ///
    /// IMPORTANT: Keep this `false` (the default) on memory-constrained deployments.
    /// Each hardening scan launches a full `nix eval` subprocess that can consume
    /// several GiB of memory.  When `false`, scans must be triggered manually via
    /// the API; the durable `crystal-forge-hardening` queue worker still runs and
    /// will pick up any manually-enqueued scans.
    ///
    /// Setting this to `true` caused a production OOM incident on 2026-07-28:
    /// nine concurrent hardening `nix eval` processes overlapped a bulk
    /// `nix-eval-jobs` evaluation, driving the server cgroup to 58.5 GiB with
    /// 1.9 GiB of swap and making the API unresponsive.
    #[serde(default)]
    pub auto_hardening_scans: bool,

    /// Authentication mode: "dev" or "oidc"
    /// Default: "oidc" (read from AUTH_MODE env var)
    #[serde(default = "default_auth_mode")]
    pub auth_mode: String,

    /// Eval/build execution mode.
    /// - real: uses nix-eval-jobs and nix build paths
    /// - mock: deterministic dev-only mock execution
    #[serde(default)]
    pub execution_mode: ExecutionMode,

    /// Whether to allow new user registration (for local auth mode).
    /// When false, only the initial admin can be registered (when no users exist).
    /// Default: false
    #[serde(default)]
    pub allow_registration: bool,

    /// Maximum total log size stored per build job in MB.
    /// Default: 10 MB.
    #[serde(default = "default_max_build_log_size_mb")]
    pub max_build_log_size_mb: usize,

    /// Maximum size per append logs request payload in MB.
    /// Default: 1 MB.
    #[serde(default = "default_max_build_log_chunk_mb")]
    pub max_build_log_chunk_mb: usize,

    /// Retention period for successful build job logs in days.
    /// Older logs are cleared by background retention task.
    /// Default: 30 days.
    #[serde(default = "default_build_log_retention_days")]
    pub build_log_retention_days: i32,

    /// Retention period for failed build job logs in days.
    /// Older logs are cleared by background retention task.
    /// Default: 90 days.
    #[serde(default = "default_failed_build_log_retention_days")]
    pub failed_build_log_retention_days: i32,

    /// Retention period for cached commit metadata in days.
    /// Older cache entries are cleared by background garbage collection task.
    /// Default: 30 days.
    #[serde(default = "default_commit_cache_retention_days")]
    pub commit_cache_retention_days: i32,

    /// Allow cache credential-test endpoint to probe private/non-routable targets.
    /// Default: false (secure-by-default SSRF posture).
    #[serde(default)]
    pub allow_private_cache_test_targets: bool,

    /// Allow credential-bearing builder cache-push config to be delivered when
    /// the request arrives via a reverse proxy that sets X-Forwarded-Proto /
    /// Forwarded headers asserting HTTPS.
    ///
    /// **Only enable this when your deployment proxy unconditionally strips and
    /// re-sets these headers itself.** A builder that can reach the server
    /// directly over plaintext HTTP could otherwise spoof the header and
    /// receive real cache credentials.
    ///
    /// When false (the default), credential-bearing cache-push config is never
    /// sent to builders — the server returns 426 Upgrade Required regardless of
    /// any forwarded-proto header.
    #[serde(default)]
    pub trust_forwarded_builder_https: bool,

    /// Default remote build execution strategy for API builders.
    /// Defaults to `server_derivation`; set to `source_re_evaluate_verified`
    /// only for builders explicitly configured with source access/capability.
    /// Builders must advertise support for whichever strategy is selected.
    #[serde(default = "default_remote_build_execution_strategy")]
    pub remote_build_execution_strategy: RemoteBuildExecutionStrategy,

    /// Default agent heartbeat interval in seconds returned via LogResponse when a system
    /// has no per-system heartbeat_interval_secs configured (systems.heartbeat_interval_secs IS NULL).
    /// Agents fall back to their compiled-in 600s default when this field is absent from the
    /// server response, so changing this only affects agents that have checked in after the
    /// server was updated.
    /// Default: 600 (10 minutes).
    #[serde(default = "default_heartbeat_interval_secs")]
    pub heartbeat_interval_secs: u64,

    /// Root directory for caching bare Git mirrors used to generate source
    /// archives for ServerBundledArchive delivery mode.
    /// Default: /var/lib/crystal-forge/source-archives
    #[serde(default = "default_source_archive_root")]
    pub source_archive_root: PathBuf,

    /// Default source/input delivery mode for verified source re-evaluation.
    /// - `local_git_worktree` (default): builder clones/fetches the repo directly
    ///   and creates a local worktree.
    /// - `server_bundled_archive`: server generates a tar archive of its bare
    ///   mirror and serves it via an authenticated API endpoint. Builders do not
    ///   need direct Git remote access.
    #[serde(default = "default_source_delivery_mode")]
    pub source_delivery_mode: SourceInputDeliveryMode,

    /// Enables outbound notification email when transport details are also
    /// configured and external delivery is permitted. Disabled by default so
    /// newly deployed instances never send unsolicited email.
    #[serde(default)]
    pub notification_email_enabled: bool,

    /// Deployment classification/external-delivery policy gate for email.
    /// Keep this false on restricted deployments even if SMTP is configured.
    #[serde(default)]
    pub notification_email_external_delivery_allowed: bool,

    /// HTTPS endpoint used by the notification email HTTP provider worker.
    #[serde(default)]
    pub notification_email_endpoint: Option<String>,

    /// Public Crystal Forge origin used to expand application routes in email.
    #[serde(default)]
    pub public_base_url: Option<String>,

    /// Allow http:// provider endpoints only for loopback development.
    #[serde(default)]
    pub notification_email_allow_insecure_loopback: bool,

    /// File containing the provider bearer token. The token is never accepted inline.
    #[serde(default)]
    pub notification_email_provider_token_file: Option<PathBuf>,

    /// Sender email address used for notification email.
    #[serde(default)]
    pub notification_email_sender_address: Option<String>,

    /// Human-readable sender display name.
    #[serde(default = "default_notification_email_sender_name")]
    pub notification_email_sender_name: String,

    /// Poll interval for immediate notification delivery worker.
    #[serde(default = "default_notification_email_worker_interval_seconds")]
    pub notification_email_worker_interval_seconds: u64,

    /// Maximum send attempts before a delivery is permanently failed.
    #[serde(default = "default_notification_email_max_attempts")]
    pub notification_email_max_attempts: i32,

    /// HTTP provider request timeout in seconds.
    #[serde(default = "default_notification_email_request_timeout_seconds")]
    pub notification_email_request_timeout_seconds: u64,

    /// Digest schedule. Currently supports the previous completed UTC week.
    #[serde(default = "default_notification_email_digest_schedule")]
    pub notification_email_digest_schedule: String,

    /// Session last-seen update throttle in seconds.
    #[serde(default = "default_session_last_seen_throttle_seconds")]
    pub session_last_seen_throttle_seconds: u64,

    /// Retention period for expired/revoked session rows.
    #[serde(default = "default_session_retention_days")]
    pub session_retention_days: i32,
}

fn default_remote_build_execution_strategy() -> RemoteBuildExecutionStrategy {
    RemoteBuildExecutionStrategy::ServerDerivation
}

fn default_heartbeat_interval_secs() -> u64 {
    600 // 10 minutes — matches the agent's compiled-in fallback
}

fn default_source_archive_root() -> PathBuf {
    PathBuf::from("/var/lib/crystal-forge/source-archives")
}

fn default_source_delivery_mode() -> SourceInputDeliveryMode {
    SourceInputDeliveryMode::LocalGitWorktree
}

fn default_notification_email_sender_name() -> String {
    "Crystal Forge".to_string()
}

fn default_notification_email_worker_interval_seconds() -> u64 {
    60
}

fn default_notification_email_max_attempts() -> i32 {
    5
}

fn default_notification_email_request_timeout_seconds() -> u64 {
    30
}

fn default_notification_email_digest_schedule() -> String {
    "weekly_utc".to_string()
}

fn default_session_last_seen_throttle_seconds() -> u64 {
    300
}

fn default_session_retention_days() -> i32 {
    30
}

// Default value functions for serde
fn default_eval_workers() -> usize {
    2 // Conservative: don't hose the system by default
}

fn default_eval_max_memory_mb() -> usize {
    4096 // 4 GB per worker
}

fn default_eval_check_cache() -> bool {
    true // Usually helpful for build planning
}

fn default_auth_mode() -> String {
    std::env::var("AUTH_MODE").unwrap_or_else(|_| "oidc".to_string())
}

fn default_max_build_log_size_mb() -> usize {
    10
}

fn default_max_build_log_chunk_mb() -> usize {
    1
}

fn default_build_log_retention_days() -> i32 {
    30
}

fn default_failed_build_log_retention_days() -> i32 {
    90
}

fn default_commit_cache_retention_days() -> i32 {
    30
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 3000,
            eval_workers: default_eval_workers(),
            eval_max_memory_mb: default_eval_max_memory_mb(),
            eval_check_cache: default_eval_check_cache(),
            auto_hardening_scans: false,
            auth_mode: default_auth_mode(),
            execution_mode: ExecutionMode::default(),
            allow_registration: false,
            max_build_log_size_mb: default_max_build_log_size_mb(),
            max_build_log_chunk_mb: default_max_build_log_chunk_mb(),
            build_log_retention_days: default_build_log_retention_days(),
            failed_build_log_retention_days: default_failed_build_log_retention_days(),
            commit_cache_retention_days: default_commit_cache_retention_days(),
            allow_private_cache_test_targets: false,
            trust_forwarded_builder_https: false,
            remote_build_execution_strategy: default_remote_build_execution_strategy(),
            heartbeat_interval_secs: default_heartbeat_interval_secs(),
            source_archive_root: default_source_archive_root(),
            source_delivery_mode: default_source_delivery_mode(),
            notification_email_enabled: false,
            notification_email_external_delivery_allowed: false,
            notification_email_endpoint: None,
            public_base_url: None,
            notification_email_allow_insecure_loopback: false,
            notification_email_provider_token_file: None,
            notification_email_sender_address: None,
            notification_email_sender_name: default_notification_email_sender_name(),
            notification_email_worker_interval_seconds:
                default_notification_email_worker_interval_seconds(),
            notification_email_max_attempts: default_notification_email_max_attempts(),
            notification_email_request_timeout_seconds:
                default_notification_email_request_timeout_seconds(),
            notification_email_digest_schedule: default_notification_email_digest_schedule(),
            session_last_seen_throttle_seconds: default_session_last_seen_throttle_seconds(),
            session_retention_days: default_session_retention_days(),
        }
    }
}

impl ServerConfig {
    /// Returns the full socket address to bind to.
    pub fn bind_address(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }

    /// Get arguments for nix-eval-jobs based on config.
    pub fn nix_eval_jobs_args(&self) -> Vec<String> {
        let mut args = vec![
            "--workers".to_string(),
            self.eval_workers.to_string(),
            "--max-memory-size".to_string(),
            self.eval_max_memory_mb.to_string(),
        ];

        if self.eval_check_cache {
            args.push("--check-cache-status".to_string());
        }

        args
    }

    /// Validate configuration and warn about potential issues.
    pub fn validate(&self) -> Result<(), String> {
        // Check for excessive memory allocation
        let total_eval_memory_mb = self.eval_workers * self.eval_max_memory_mb;
        if total_eval_memory_mb > 32768 {
            // 32 GB
            return Err(format!(
                "Evaluation memory too high: {} workers × {} MB = {} MB total ({}GB). \
                 This may exhaust system memory.",
                self.eval_workers,
                self.eval_max_memory_mb,
                total_eval_memory_mb,
                total_eval_memory_mb / 1024
            ));
        }

        // Warn if eval workers seems excessive
        if self.eval_workers > 16 {
            eprintln!(
                "⚠️  Warning: {} evaluation workers is very high. \
                 Consider reducing to 4-8 for most systems.",
                self.eval_workers
            );
        }

        if self.max_build_log_chunk_mb == 0 {
            return Err("max_build_log_chunk_mb must be greater than 0".to_string());
        }

        if self.max_build_log_size_mb < self.max_build_log_chunk_mb {
            return Err(format!(
                "max_build_log_size_mb ({}) must be >= max_build_log_chunk_mb ({})",
                self.max_build_log_size_mb, self.max_build_log_chunk_mb
            ));
        }

        if self.build_log_retention_days <= 0 || self.failed_build_log_retention_days <= 0 {
            return Err("build log retention days must be greater than 0".to_string());
        }

        if self.notification_email_enabled {
            let endpoint = self
                .notification_email_endpoint
                .as_deref()
                .unwrap_or("")
                .trim();
            if endpoint.is_empty() {
                return Err(
                    "notification_email_endpoint is required when notification email is enabled"
                        .to_string(),
                );
            }
            if !endpoint.starts_with("https://") && !endpoint.starts_with("http://") {
                return Err(
                    "notification_email_endpoint must be an HTTP provider URL starting with http:// or https://"
                        .to_string(),
                );
            }
            if endpoint.starts_with("http://")
                && !(self.notification_email_allow_insecure_loopback
                    && is_loopback_http_url(endpoint))
            {
                return Err(
                    "notification_email_endpoint must use https:// unless notification_email_allow_insecure_loopback is true and the endpoint host is loopback"
                        .to_string(),
                );
            }
            let public_base_url = self.public_base_url.as_deref().unwrap_or("").trim();
            if !is_safe_public_base_url(public_base_url) {
                return Err(
                    "public_base_url is required for email and must be an https:// origin without path, query, or fragment"
                        .to_string(),
                );
            }
            match self.notification_email_provider_token_file.as_ref() {
                Some(path) if path.starts_with("/nix/store") => {
                    return Err(
                        "notification_email_provider_token_file must not point into /nix/store"
                            .to_string(),
                    );
                }
                Some(path) if path.as_os_str().is_empty() => {
                    return Err(
                        "notification_email_provider_token_file must not be empty when email is enabled"
                            .to_string(),
                    );
                }
                Some(_) => {}
                None => {
                    return Err(
                        "notification_email_provider_token_file is required when notification email is enabled"
                            .to_string(),
                    );
                }
            }
            if self
                .notification_email_sender_address
                .as_deref()
                .unwrap_or("")
                .trim()
                .is_empty()
            {
                return Err(
                    "notification_email_sender_address is required when notification email is enabled"
                        .to_string(),
                );
            }
        }

        if self.notification_email_worker_interval_seconds == 0 {
            return Err("notification email worker interval must be greater than 0".to_string());
        }

        if self.notification_email_max_attempts <= 0 {
            return Err("notification email max attempts must be greater than 0".to_string());
        }

        if self.notification_email_request_timeout_seconds == 0 {
            return Err("notification email request timeout must be greater than 0".to_string());
        }

        if self.notification_email_digest_schedule != "weekly_utc" {
            return Err("notification email digest schedule must be weekly_utc".to_string());
        }

        if self.session_last_seen_throttle_seconds == 0 {
            return Err("session last-seen throttle must be greater than 0".to_string());
        }

        if self.session_retention_days <= 0 {
            return Err("session retention days must be greater than 0".to_string());
        }

        if self.execution_mode.is_mock() && self.auth_mode != "local" {
            return Err(
                "server.execution_mode=mock requires server.auth_mode=local for safety".to_string(),
            );
        }

        // Validate heartbeat interval is within acceptable range
        const MIN_HEARTBEAT_INTERVAL_SECS: u64 = 15;
        const MAX_HEARTBEAT_INTERVAL_SECS: u64 = 900;
        if self.heartbeat_interval_secs < MIN_HEARTBEAT_INTERVAL_SECS
            || self.heartbeat_interval_secs > MAX_HEARTBEAT_INTERVAL_SECS
        {
            return Err(format!(
                "heartbeat_interval_secs ({}) must be between {} and {} seconds",
                self.heartbeat_interval_secs,
                MIN_HEARTBEAT_INTERVAL_SECS,
                MAX_HEARTBEAT_INTERVAL_SECS
            ));
        }

        Ok(())
    }
}

fn is_loopback_http_url(value: &str) -> bool {
    let Some(rest) = value.strip_prefix("http://") else {
        return false;
    };
    let host_port = rest.split('/').next().unwrap_or_default();
    let host = host_port
        .strip_prefix('[')
        .and_then(|value| value.split(']').next())
        .unwrap_or_else(|| host_port.split(':').next().unwrap_or_default());
    matches!(host, "localhost" | "127.0.0.1" | "::1") || host.starts_with("127.")
}

fn is_safe_public_base_url(value: &str) -> bool {
    let Some(rest) = value.strip_prefix("https://") else {
        return false;
    };
    !rest.is_empty()
        && !rest.contains('/')
        && !rest.contains('?')
        && !rest.contains('#')
        && !rest.contains('@')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn execution_mode_defaults_to_real() {
        let cfg = ServerConfig::default();
        assert_eq!(cfg.execution_mode, ExecutionMode::Real);
    }

    #[test]
    fn remote_build_strategy_defaults_to_server_derivation() {
        let cfg = ServerConfig::default();
        assert_eq!(
            cfg.remote_build_execution_strategy,
            RemoteBuildExecutionStrategy::ServerDerivation
        );
    }

    #[test]
    fn mock_mode_requires_local_auth_mode() {
        let mut cfg = ServerConfig::default();
        cfg.execution_mode = ExecutionMode::Mock;
        cfg.auth_mode = "oidc".to_string();

        let err = cfg
            .validate()
            .expect_err("mock mode should require auth_mode=local");
        assert!(err.contains("execution_mode=mock requires server.auth_mode=local"));
    }

    #[test]
    fn mock_mode_allows_local_auth_mode() {
        let mut cfg = ServerConfig::default();
        cfg.execution_mode = ExecutionMode::Mock;
        cfg.auth_mode = "local".to_string();
        cfg.validate()
            .expect("mock mode should be allowed in local auth mode");
    }

    #[test]
    fn trust_forwarded_builder_https_defaults_false() {
        let cfg = ServerConfig::default();
        assert!(
            !cfg.trust_forwarded_builder_https,
            "credential delivery must be opt-in, not opt-out"
        );
    }

    #[test]
    fn automatic_hardening_scans_default_false() {
        let cfg = ServerConfig::default();
        assert!(
            !cfg.auto_hardening_scans,
            "expensive hardening evaluations must require explicit opt-in"
        );
    }

    #[test]
    fn source_archive_root_defaults_to_expected_path() {
        let cfg = ServerConfig::default();
        assert_eq!(
            cfg.source_archive_root,
            PathBuf::from("/var/lib/crystal-forge/source-archives")
        );
    }

    #[test]
    fn source_delivery_mode_defaults_to_local_git_worktree() {
        let cfg = ServerConfig::default();
        assert_eq!(
            cfg.source_delivery_mode,
            SourceInputDeliveryMode::LocalGitWorktree
        );
    }
}
