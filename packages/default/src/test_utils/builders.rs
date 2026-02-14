//! Type-safe builders for domain types used in tests.
//!
//! Each builder:
//! - Starts with `::new()` (no arguments required).
//! - Provides setter methods that return `&mut Self` for chaining.
//! - Produces a fully valid instance via `.build()` with sensible defaults.
//!
//! # Design Principles
//!
//! 1. **Zero-config validity** — `FooBuilder::new().build()` always compiles
//!    and returns a struct that won't trip runtime validation.
//! 2. **Discoverable** — setters mirror the target struct's field names.
//! 3. **Composable** — builders for nested types (e.g. `CveSummary` inside
//!    `DashboardSummary`) can be passed into parent builders.

use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::api::models::{
    CveSummary, DashboardSummary, DeploymentStatus, DeploymentStatusSummary, FleetHealthSummary,
    HealthStatus, PipelineStage, RecentDeployment, SystemSummary,
};
use crate::derivations::{Derivation, DerivationType};
use crate::models::commits::Commit;
use crate::models::flakes::Flake;
use crate::models::system_states::SystemState;

// ─────────────────────────────────────────────────────────────────────────────
// Derivation
// ─────────────────────────────────────────────────────────────────────────────

/// Builder for [`Derivation`].
#[derive(Debug)]
pub struct DerivationBuilder {
    id: i32,
    commit_id: Option<i32>,
    derivation_type: DerivationType,
    derivation_name: String,
    derivation_path: Option<String>,
    scheduled_at: Option<DateTime<Utc>>,
    completed_at: Option<DateTime<Utc>>,
    started_at: Option<DateTime<Utc>>,
    attempt_count: i32,
    evaluation_duration_ms: Option<i32>,
    error_message: Option<String>,
    pname: Option<String>,
    version: Option<String>,
    status_id: i32,
    derivation_target: Option<String>,
    build_elapsed_seconds: Option<i32>,
    build_current_target: Option<String>,
    build_last_activity_seconds: Option<i32>,
    build_last_heartbeat: Option<DateTime<Utc>>,
    cf_agent_enabled: Option<bool>,
    store_path: Option<String>,
}

impl DerivationBuilder {
    pub fn new() -> Self {
        Self {
            id: 1,
            commit_id: Some(1),
            derivation_type: DerivationType::NixOS,
            derivation_name: "test-system".into(),
            derivation_path: None,
            scheduled_at: None,
            completed_at: None,
            started_at: None,
            attempt_count: 0,
            evaluation_duration_ms: None,
            error_message: None,
            pname: None,
            version: None,
            status_id: 1,
            derivation_target: None,
            build_elapsed_seconds: None,
            build_current_target: None,
            build_last_activity_seconds: None,
            build_last_heartbeat: None,
            cf_agent_enabled: None,
            store_path: None,
        }
    }

    pub fn id(&mut self, id: i32) -> &mut Self {
        self.id = id;
        self
    }
    pub fn commit_id(&mut self, commit_id: Option<i32>) -> &mut Self {
        self.commit_id = commit_id;
        self
    }
    pub fn derivation_type(&mut self, dt: DerivationType) -> &mut Self {
        self.derivation_type = dt;
        self
    }
    pub fn name(&mut self, name: &str) -> &mut Self {
        self.derivation_name = name.into();
        self
    }
    pub fn path(&mut self, path: &str) -> &mut Self {
        self.derivation_path = Some(path.into());
        self
    }
    pub fn status_id(&mut self, id: i32) -> &mut Self {
        self.status_id = id;
        self
    }
    pub fn store_path(&mut self, path: &str) -> &mut Self {
        self.store_path = Some(path.into());
        self
    }
    pub fn cf_agent_enabled(&mut self, enabled: bool) -> &mut Self {
        self.cf_agent_enabled = Some(enabled);
        self
    }
    pub fn error_message(&mut self, msg: &str) -> &mut Self {
        self.error_message = Some(msg.into());
        self
    }
    pub fn attempt_count(&mut self, count: i32) -> &mut Self {
        self.attempt_count = count;
        self
    }
    pub fn completed_at(&mut self, ts: DateTime<Utc>) -> &mut Self {
        self.completed_at = Some(ts);
        self
    }
    pub fn started_at(&mut self, ts: DateTime<Utc>) -> &mut Self {
        self.started_at = Some(ts);
        self
    }
    pub fn pname(&mut self, pname: &str) -> &mut Self {
        self.pname = Some(pname.into());
        self
    }
    pub fn version(&mut self, version: &str) -> &mut Self {
        self.version = Some(version.into());
        self
    }
    pub fn derivation_target(&mut self, target: &str) -> &mut Self {
        self.derivation_target = Some(target.into());
        self
    }

    pub fn build(&self) -> Derivation {
        Derivation {
            id: self.id,
            commit_id: self.commit_id,
            derivation_type: self.derivation_type.clone(),
            derivation_name: self.derivation_name.clone(),
            derivation_path: self.derivation_path.clone(),
            scheduled_at: self.scheduled_at,
            completed_at: self.completed_at,
            started_at: self.started_at,
            attempt_count: self.attempt_count,
            evaluation_duration_ms: self.evaluation_duration_ms,
            error_message: self.error_message.clone(),
            pname: self.pname.clone(),
            version: self.version.clone(),
            status_id: self.status_id,
            derivation_target: self.derivation_target.clone(),
            build_elapsed_seconds: self.build_elapsed_seconds,
            build_current_target: self.build_current_target.clone(),
            build_last_activity_seconds: self.build_last_activity_seconds,
            build_last_heartbeat: self.build_last_heartbeat,
            cf_agent_enabled: self.cf_agent_enabled,
            store_path: self.store_path.clone(),
        }
    }
}

impl Default for DerivationBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Commit
// ─────────────────────────────────────────────────────────────────────────────

/// Builder for [`Commit`].
#[derive(Debug)]
pub struct CommitBuilder {
    id: i32,
    flake_id: i32,
    git_commit_hash: String,
    commit_timestamp: DateTime<Utc>,
    attempt_count: i32,
}

impl CommitBuilder {
    pub fn new() -> Self {
        Self {
            id: 1,
            flake_id: 1,
            git_commit_hash: "abc123def456".into(),
            commit_timestamp: Utc::now(),
            attempt_count: 0,
        }
    }

    pub fn id(&mut self, id: i32) -> &mut Self {
        self.id = id;
        self
    }
    pub fn flake_id(&mut self, id: i32) -> &mut Self {
        self.flake_id = id;
        self
    }
    pub fn hash(&mut self, hash: &str) -> &mut Self {
        self.git_commit_hash = hash.into();
        self
    }
    pub fn timestamp(&mut self, ts: DateTime<Utc>) -> &mut Self {
        self.commit_timestamp = ts;
        self
    }
    pub fn attempt_count(&mut self, count: i32) -> &mut Self {
        self.attempt_count = count;
        self
    }

    pub fn build(&self) -> Commit {
        Commit {
            id: self.id,
            flake_id: self.flake_id,
            git_commit_hash: self.git_commit_hash.clone(),
            commit_timestamp: self.commit_timestamp,
            attempt_count: self.attempt_count,
        }
    }
}

impl Default for CommitBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Flake
// ─────────────────────────────────────────────────────────────────────────────

/// Builder for [`Flake`].
#[derive(Debug)]
pub struct FlakeBuilder {
    id: i32,
    name: String,
    repo_url: String,
}

impl FlakeBuilder {
    pub fn new() -> Self {
        Self {
            id: 1,
            name: "test-flake".into(),
            repo_url: "https://github.com/example/test-flake.git".into(),
        }
    }

    pub fn id(&mut self, id: i32) -> &mut Self {
        self.id = id;
        self
    }
    pub fn name(&mut self, name: &str) -> &mut Self {
        self.name = name.into();
        self
    }
    pub fn repo_url(&mut self, url: &str) -> &mut Self {
        self.repo_url = url.into();
        self
    }

    pub fn build(&self) -> Flake {
        Flake {
            id: self.id,
            name: self.name.clone(),
            repo_url: self.repo_url.clone(),
        }
    }
}

impl Default for FlakeBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// SystemState
// ─────────────────────────────────────────────────────────────────────────────

/// Builder for [`SystemState`].
///
/// Produces a complete, realistic `SystemState` with sensible defaults for
/// every field — hardware IDs, network, security, and software identity.
#[derive(Debug)]
pub struct SystemStateBuilder {
    hostname: String,
    change_reason: String,
    store_path: Option<String>,
    os: Option<String>,
    kernel: Option<String>,
    memory_gb: Option<f64>,
    cpu_cores: Option<i32>,
    agent_version: Option<String>,
    nixos_version: Option<String>,
    agent_compatible: Option<bool>,
    partial_data: Option<bool>,
}

impl SystemStateBuilder {
    pub fn new() -> Self {
        Self {
            hostname: "test-host".into(),
            change_reason: "startup".into(),
            store_path: Some("/nix/store/test-system-path".into()),
            os: Some("25.11".into()),
            kernel: Some("6.12.33".into()),
            memory_gb: Some(16.0),
            cpu_cores: Some(4),
            agent_version: Some("0.2.1-test".into()),
            nixos_version: Some("25.11".into()),
            agent_compatible: Some(true),
            partial_data: Some(false),
        }
    }

    pub fn hostname(&mut self, hostname: &str) -> &mut Self {
        self.hostname = hostname.into();
        self
    }
    pub fn change_reason(&mut self, reason: &str) -> &mut Self {
        self.change_reason = reason.into();
        self
    }
    pub fn store_path(&mut self, path: &str) -> &mut Self {
        self.store_path = Some(path.into());
        self
    }
    pub fn agent_compatible(&mut self, compat: bool) -> &mut Self {
        self.agent_compatible = Some(compat);
        self
    }
    pub fn partial_data(&mut self, partial: bool) -> &mut Self {
        self.partial_data = Some(partial);
        self
    }

    pub fn build(&self) -> SystemState {
        SystemState {
            id: None,
            hostname: self.hostname.clone(),
            change_reason: self.change_reason.clone(),
            timestamp: Some(Utc::now()),
            store_path: self.store_path.clone(),
            os: self.os.clone(),
            kernel: self.kernel.clone(),
            memory_gb: self.memory_gb,
            uptime_secs: Some(86400),
            cpu_brand: Some("Test CPU".into()),
            cpu_cores: self.cpu_cores,
            board_serial: Some("TEST-SERIAL-001".into()),
            product_uuid: Some(format!("test-uuid-{}", self.hostname)),
            rootfs_uuid: Some(format!("test-rootfs-{}", self.hostname)),
            chassis_serial: Some("CHASSIS-001".into()),
            bios_version: Some("1.0.0".into()),
            cpu_microcode: None,
            network_interfaces: Some(serde_json::Value::Array(vec![])),
            primary_mac_address: Some("02:00:00:00:00:01".into()),
            primary_ip_address: Some("192.168.1.100".into()),
            gateway_ip: Some("192.168.1.1".into()),
            selinux_status: None,
            tpm_present: Some(true),
            secure_boot_enabled: Some(false),
            fips_mode: Some(false),
            agent_version: self.agent_version.clone(),
            agent_build_hash: Some("test-build".into()),
            nixos_version: self.nixos_version.clone(),
            agent_compatible: self.agent_compatible,
            partial_data: self.partial_data,
        }
    }
}

impl Default for SystemStateBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// API DTO builders
// ─────────────────────────────────────────────────────────────────────────────

/// Builder for [`DashboardSummary`].
#[derive(Debug)]
pub struct DashboardSummaryBuilder {
    fleet_health: FleetHealthSummary,
    deployment_status: DeploymentStatusSummary,
    cve_summary: CveSummary,
    total_systems: i64,
    active_builds: i64,
    recent_deployments: Vec<RecentDeployment>,
}

impl DashboardSummaryBuilder {
    pub fn new() -> Self {
        Self {
            fleet_health: FleetHealthSummary {
                healthy: 5,
                warning: 1,
                critical: 0,
                offline: 0,
            },
            deployment_status: DeploymentStatusSummary {
                up_to_date: 4,
                behind: 1,
                never_deployed: 1,
                unknown: 0,
            },
            cve_summary: CveSummary {
                critical: 0,
                high: 2,
                medium: 8,
                low: 15,
            },
            total_systems: 6,
            active_builds: 1,
            recent_deployments: vec![],
        }
    }

    pub fn total_systems(&mut self, n: i64) -> &mut Self {
        self.total_systems = n;
        self
    }
    pub fn active_builds(&mut self, n: i64) -> &mut Self {
        self.active_builds = n;
        self
    }
    pub fn fleet_health(&mut self, fh: FleetHealthSummary) -> &mut Self {
        self.fleet_health = fh;
        self
    }
    pub fn cve_summary(&mut self, cs: CveSummary) -> &mut Self {
        self.cve_summary = cs;
        self
    }

    pub fn build(&self) -> DashboardSummary {
        DashboardSummary {
            fleet_health: self.fleet_health.clone(),
            deployment_status: self.deployment_status.clone(),
            cve_summary: self.cve_summary.clone(),
            total_systems: self.total_systems,
            active_builds: self.active_builds,
            recent_deployments: self.recent_deployments.clone(),
            timestamp: Utc::now(),
        }
    }
}

impl Default for DashboardSummaryBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Builder for [`SystemSummary`].
#[derive(Debug)]
pub struct SystemSummaryBuilder {
    id: Uuid,
    hostname: String,
    environment: Option<String>,
    health_status: HealthStatus,
    deployment_status: DeploymentStatus,
    pipeline_stage: Option<PipelineStage>,
    cve_counts: CveSummary,
    nixos_version: Option<String>,
    last_seen: Option<DateTime<Utc>>,
    deployment_policy: String,
}

impl SystemSummaryBuilder {
    pub fn new() -> Self {
        Self {
            id: Uuid::new_v4(),
            hostname: "test-host".into(),
            environment: Some("production".into()),
            health_status: HealthStatus::Healthy,
            deployment_status: DeploymentStatus::UpToDate,
            pipeline_stage: None,
            cve_counts: CveSummary {
                critical: 0,
                high: 0,
                medium: 0,
                low: 0,
            },
            nixos_version: Some("25.11".into()),
            last_seen: Some(Utc::now()),
            deployment_policy: "manual".into(),
        }
    }

    pub fn hostname(&mut self, hostname: &str) -> &mut Self {
        self.hostname = hostname.into();
        self
    }
    pub fn health_status(&mut self, status: HealthStatus) -> &mut Self {
        self.health_status = status;
        self
    }
    pub fn deployment_status(&mut self, status: DeploymentStatus) -> &mut Self {
        self.deployment_status = status;
        self
    }
    pub fn deployment_policy(&mut self, policy: &str) -> &mut Self {
        self.deployment_policy = policy.into();
        self
    }

    pub fn build(&self) -> SystemSummary {
        SystemSummary {
            id: self.id,
            hostname: self.hostname.clone(),
            environment: self.environment.clone(),
            health_status: self.health_status,
            deployment_status: self.deployment_status,
            pipeline_stage: self.pipeline_stage,
            cve_counts: self.cve_counts.clone(),
            nixos_version: self.nixos_version.clone(),
            last_seen: self.last_seen,
            deployment_policy: self.deployment_policy.clone(),
        }
    }
}

impl Default for SystemSummaryBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests — TDD: these verify the builders themselves behave correctly.
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Derivation ──────────────────────────────────────────────────────

    #[test]
    fn derivation_builder_defaults_produce_valid_instance() {
        let d = DerivationBuilder::new().build();
        assert_eq!(d.id, 1);
        assert_eq!(d.derivation_name, "test-system");
        assert!(matches!(d.derivation_type, DerivationType::NixOS));
        assert_eq!(d.attempt_count, 0);
    }

    #[test]
    fn derivation_builder_overrides_apply() {
        let d = DerivationBuilder::new()
            .id(42)
            .name("web-server")
            .derivation_type(DerivationType::Package)
            .store_path("/nix/store/abc")
            .cf_agent_enabled(true)
            .build();

        assert_eq!(d.id, 42);
        assert_eq!(d.derivation_name, "web-server");
        assert!(matches!(d.derivation_type, DerivationType::Package));
        assert_eq!(d.store_path.as_deref(), Some("/nix/store/abc"));
        assert_eq!(d.cf_agent_enabled, Some(true));
    }

    #[test]
    fn derivation_is_deployable_requires_nixos_and_agent() {
        // NixOS + agent enabled → deployable
        let d = DerivationBuilder::new().cf_agent_enabled(true).build();
        assert!(d.is_deployable());

        // Package type → not deployable even with agent
        let d = DerivationBuilder::new()
            .derivation_type(DerivationType::Package)
            .cf_agent_enabled(true)
            .build();
        assert!(!d.is_deployable());

        // NixOS but no agent → not deployable
        let d = DerivationBuilder::new().cf_agent_enabled(false).build();
        assert!(!d.is_deployable());
    }

    // ── Commit ──────────────────────────────────────────────────────────

    #[test]
    fn commit_builder_defaults_produce_valid_instance() {
        let c = CommitBuilder::new().build();
        assert_eq!(c.id, 1);
        assert_eq!(c.flake_id, 1);
        assert_eq!(c.git_commit_hash, "abc123def456");
        assert_eq!(c.attempt_count, 0);
    }

    #[test]
    fn commit_builder_overrides_apply() {
        let c = CommitBuilder::new()
            .id(7)
            .flake_id(3)
            .hash("deadbeef")
            .attempt_count(2)
            .build();

        assert_eq!(c.id, 7);
        assert_eq!(c.flake_id, 3);
        assert_eq!(c.git_commit_hash, "deadbeef");
        assert_eq!(c.attempt_count, 2);
    }

    // ── Flake ───────────────────────────────────────────────────────────

    #[test]
    fn flake_builder_defaults_produce_valid_instance() {
        let f = FlakeBuilder::new().build();
        assert_eq!(f.id, 1);
        assert_eq!(f.name, "test-flake");
        assert!(f.repo_url.starts_with("https://"));
    }

    #[test]
    fn flake_builder_overrides_apply() {
        let f = FlakeBuilder::new()
            .id(5)
            .name("infra")
            .repo_url("git@github.com:org/infra.git")
            .build();

        assert_eq!(f.id, 5);
        assert_eq!(f.name, "infra");
        assert_eq!(f.repo_url, "git@github.com:org/infra.git");
    }

    // ── SystemState ─────────────────────────────────────────────────────

    #[test]
    fn system_state_builder_defaults_produce_valid_instance() {
        let s = SystemStateBuilder::new().build();
        assert_eq!(s.hostname, "test-host");
        assert_eq!(s.change_reason, "startup");
        assert!(s.store_path.is_some());
        assert_eq!(s.agent_compatible, Some(true));
        assert_eq!(s.partial_data, Some(false));
        // Hardware fields populated
        assert!(s.cpu_cores.is_some());
        assert!(s.memory_gb.is_some());
    }

    #[test]
    fn system_state_builder_overrides_apply() {
        let s = SystemStateBuilder::new()
            .hostname("db-primary")
            .change_reason("config_change")
            .agent_compatible(false)
            .build();

        assert_eq!(s.hostname, "db-primary");
        assert_eq!(s.change_reason, "config_change");
        assert_eq!(s.agent_compatible, Some(false));
    }

    #[test]
    fn system_state_builder_product_uuid_includes_hostname() {
        let s = SystemStateBuilder::new().hostname("web-1").build();
        assert_eq!(s.product_uuid.as_deref(), Some("test-uuid-web-1"));
    }

    // ── DashboardSummary ────────────────────────────────────────────────

    #[test]
    fn dashboard_summary_builder_defaults_produce_valid_instance() {
        let d = DashboardSummaryBuilder::new().build();
        assert_eq!(d.total_systems, 6);
        assert_eq!(d.active_builds, 1);
        assert_eq!(d.fleet_health.total(), 6);
        assert_eq!(d.cve_summary.total(), 25);
    }

    #[test]
    fn dashboard_summary_builder_overrides_apply() {
        let d = DashboardSummaryBuilder::new()
            .total_systems(100)
            .active_builds(5)
            .build();

        assert_eq!(d.total_systems, 100);
        assert_eq!(d.active_builds, 5);
    }

    // ── SystemSummary ───────────────────────────────────────────────────

    #[test]
    fn system_summary_builder_defaults_produce_valid_instance() {
        let s = SystemSummaryBuilder::new().build();
        assert_eq!(s.hostname, "test-host");
        assert_eq!(s.health_status, HealthStatus::Healthy);
        assert_eq!(s.deployment_status, DeploymentStatus::UpToDate);
        assert_eq!(s.deployment_policy, "manual");
    }

    #[test]
    fn system_summary_builder_overrides_apply() {
        let s = SystemSummaryBuilder::new()
            .hostname("edge-node")
            .health_status(HealthStatus::Critical)
            .deployment_status(DeploymentStatus::Behind)
            .deployment_policy("auto_latest")
            .build();

        assert_eq!(s.hostname, "edge-node");
        assert_eq!(s.health_status, HealthStatus::Critical);
        assert_eq!(s.deployment_status, DeploymentStatus::Behind);
        assert_eq!(s.deployment_policy, "auto_latest");
    }
}
