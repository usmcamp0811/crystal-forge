//! Agent ↔ server wire protocol types.
//!
//! This module contains only serializable DTOs exchanged over HTTP between
//! Crystal Forge agents and the server. No host inspection, filesystem reads,
//! process spawning, or sysinfo collection belongs here.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Reason for a system state change/heartbeat.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ChangeReason {
    #[serde(rename = "startup")]
    Startup,
    #[serde(rename = "config_change")]
    ConfigChange,
    #[serde(rename = "state_delta")]
    StateDelta,
    #[serde(rename = "cf_deployment")]
    CfDeployment,
}

impl std::fmt::Display for ChangeReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChangeReason::Startup => write!(f, "startup"),
            ChangeReason::ConfigChange => write!(f, "config_change"),
            ChangeReason::StateDelta => write!(f, "state_delta"),
            ChangeReason::CfDeployment => write!(f, "cf_deployment"),
        }
    }
}

impl std::str::FromStr for ChangeReason {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "startup" => Ok(ChangeReason::Startup),
            "config_change" => Ok(ChangeReason::ConfigChange),
            "state_delta" => Ok(ChangeReason::StateDelta),
            "cf_deployment" => Ok(ChangeReason::CfDeployment),
            _ => Err(format!("Invalid change reason: {s}")),
        }
    }
}

/// Wire representation of the system state sent by agents to the server.
///
/// This is the serde-only protocol type. Host inspection logic
/// (`SystemState::gather()`, network helpers, `/proc`/`/sys` reads) lives
/// in `cf-agent`, not here.
///
/// The server stores the state in its DB using a separate row type that adds
/// `sqlx::FromRow`.
#[derive(Debug, Serialize, Deserialize)]
pub struct SystemState {
    // ───── Identification ─────
    pub id: Option<i32>,
    pub hostname: String,
    pub change_reason: String,
    pub timestamp: Option<DateTime<Utc>>,

    // ───── System Info ─────
    pub store_path: Option<String>,
    pub generation: Option<i32>,
    pub generation_matches_current_store_path: Option<bool>,
    pub os: Option<String>,
    pub kernel: Option<String>,
    pub memory_gb: Option<f64>,
    pub uptime_secs: Option<i64>,
    pub cpu_brand: Option<String>,
    pub cpu_cores: Option<i32>,

    // ───── Hardware IDs ─────
    pub board_serial: Option<String>,
    pub product_uuid: Option<String>,
    pub rootfs_uuid: Option<String>,
    pub chassis_serial: Option<String>,
    pub bios_version: Option<String>,
    pub cpu_microcode: Option<String>,

    // ───── Network Identity ─────
    pub network_interfaces: Option<serde_json::Value>,
    pub primary_mac_address: Option<String>,
    pub primary_ip_address: Option<String>,
    pub gateway_ip: Option<String>,

    // ───── Security & Compliance ─────
    pub selinux_status: Option<String>,
    pub tpm_present: Option<bool>,
    pub secure_boot_enabled: Option<bool>,
    pub fips_mode: Option<bool>,

    // ───── Software Identity ─────
    pub agent_version: Option<String>,
    pub agent_build_hash: Option<String>,
    pub nixos_version: Option<String>,

    // ───── Agent Compatibility ─────
    pub agent_compatible: Option<bool>,
    pub partial_data: Option<bool>,

    // ───── Reboot Detection ─────
    /// Linux kernel boot UUID from /proc/sys/kernel/random/boot_id.
    #[serde(default)]
    pub boot_id: Option<String>,
}

impl SystemState {
    /// Map legacy V1 context strings to the current change_reason vocabulary.
    pub fn map_v1_context(context: &str) -> String {
        match context {
            "agent-startup" => "startup".to_string(),
            "agent-loop" => "config_change".to_string(),
            "agent-heartbeat" => "state_delta".to_string(),
            _ => context.to_string(),
        }
    }

    /// Get the change reason as an enum.
    pub fn get_change_reason(&self) -> Result<ChangeReason, String> {
        self.change_reason.parse()
    }

    /// Set the change reason from an enum.
    pub fn set_change_reason(&mut self, reason: ChangeReason) {
        self.change_reason = reason.to_string();
    }

    /// Check if this system state represents a deployment.
    pub fn is_deployment(&self) -> bool {
        matches!(self.get_change_reason(), Ok(ChangeReason::CfDeployment))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ensure `network_interfaces` serializes as a JSON *string* (double-encoded),
    /// not as an array or object. This protects the existing wire and persisted-data
    /// contract from accidental format changes.
    #[test]
    fn network_interfaces_serializes_as_json_string() {
        let state = SystemState {
            id: None,
            hostname: "test-host".into(),
            change_reason: "startup".into(),
            timestamp: None,
            store_path: Some("/nix/store/test".into()),
            generation: None,
            generation_matches_current_store_path: None,
            os: None,
            kernel: None,
            memory_gb: None,
            uptime_secs: None,
            cpu_brand: None,
            cpu_cores: None,
            board_serial: None,
            product_uuid: None,
            rootfs_uuid: None,
            chassis_serial: None,
            bios_version: None,
            cpu_microcode: None,
            network_interfaces: Some(serde_json::Value::String(
                r#"[{"name":"eth0","mac_address":"02:00:00:00:00:01","ip_addresses":[]}]"#.into(),
            )),
            primary_mac_address: None,
            primary_ip_address: None,
            gateway_ip: None,
            selinux_status: None,
            tpm_present: None,
            secure_boot_enabled: None,
            fips_mode: None,
            agent_version: None,
            agent_build_hash: None,
            nixos_version: None,
            agent_compatible: None,
            partial_data: None,
            boot_id: None,
        };

        let value = serde_json::to_value(&state).expect("serialize system state");
        let ni = value["network_interfaces"]
            .as_str()
            .expect("network_interfaces should serialize as a JSON string");

        // The string itself should be valid JSON containing an array.
        let parsed: Vec<serde_json::Value> =
            serde_json::from_str(ni).expect("string content should be valid JSON array");
        assert_eq!(parsed.len(), 1, "expected one interface entry");
        assert_eq!(parsed[0]["name"], "eth0");
    }
}

impl std::fmt::Display for SystemState {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let uptime_days = self.uptime_secs.unwrap_or(0) / 86400;
        let uptime_hours = (self.uptime_secs.unwrap_or(0) % 86400) / 3600;

        write!(
            f,
            "✅ accepted agent: {}\n   • change_reason:      {}\n   • hostname:     {}\n   • hash:         {}\n   • os:           {}\n   • kernel:       {}\n   • memory:       {} GB\n   • uptime:       {}d {}h\n   • cpu:          {} ({})\n   • board_serial: {}\n   • uuid:         {}",
            self.hostname,
            self.change_reason,
            self.hostname,
            self.store_path.as_deref().unwrap_or("unknown"),
            self.os.as_deref().unwrap_or("unknown"),
            self.kernel.as_deref().unwrap_or("unknown"),
            self.memory_gb.unwrap_or(0.0),
            uptime_days,
            uptime_hours,
            self.cpu_brand.as_deref().unwrap_or("unknown"),
            self.cpu_cores.unwrap_or(0),
            self.board_serial.as_deref().unwrap_or("unknown"),
            self.product_uuid.as_deref().unwrap_or("unknown")
        )
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Log response (server → agent)
// ─────────────────────────────────────────────────────────────────────────────

/// Cache configuration delivered to agents in heartbeat responses.
#[derive(Serialize, Deserialize)]
pub struct RuntimeCacheConfig {
    pub cache_type: String,
    pub cache_url: String,
    pub cache_public_key: Option<String>,
    pub attic_cache_name: Option<String>,
}

/// Server response to an agent heartbeat/state POST.
#[derive(Serialize, Deserialize)]
pub struct LogResponse {
    pub desired_target: Option<String>,
    #[serde(default)]
    pub runtime_caches: Vec<RuntimeCacheConfig>,
    /// Interval in seconds the agent should sleep between heartbeats.
    /// Absent when the server cannot determine the value; agent falls back to 600s.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub heartbeat_interval_secs: Option<u64>,
}
