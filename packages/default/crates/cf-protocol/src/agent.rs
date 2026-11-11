//! Agent ↔ server wire protocol types.
//!
//! This module contains only serializable DTOs exchanged over HTTP between
//! Crystal Forge agents and the server. No host inspection, filesystem reads,
//! process spawning, or sysinfo collection belongs here.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Declares cache-read support for one authenticated heartbeat.
///
/// Missing capabilities mean no support. Version strings and build hashes do
/// not attest to support; servers must read these flags from the signed body.
#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentCapabilities {
    /// Supports explicit Niks3 public and mTLS read settings without write secrets.
    #[serde(default)]
    pub supports_niks3: bool,
    /// Supports Basic reads with a runtime-verified native exact-origin guard.
    ///
    /// This independent signed flag defaults to false for mixed-version agents.
    /// General Niks3 support does not establish Basic deserialization or safe
    /// native credential handling. Servers must require both flags for Basic.
    #[serde(default)]
    pub supports_niks3_basic_read: bool,
}

/// Extends the legacy flat system-state JSON with signed capabilities.
///
/// `state` remains flattened so old servers and agents retain the existing
/// SystemState wire shape. Capabilities are request-local, not persisted state.
///
/// # Examples
/// ```
/// use cf_protocol::agent::{AgentCapabilities, CurrentSystemRequest};
/// let request = CurrentSystemRequest {
///     state: serde_json::json!({"hostname": "host", "change_reason": "startup"}),
///     capabilities: AgentCapabilities {
///         supports_niks3: true, ..AgentCapabilities::default()
///     },
///     guest_observations: Vec::new(),
/// };
/// let json = serde_json::to_value(request).unwrap();
/// assert_eq!(json["hostname"], "host");
/// assert_eq!(json["capabilities"]["supports_niks3"], true);
/// ```
#[derive(Debug, Serialize, Deserialize)]
pub struct CurrentSystemRequest<T> {
    /// Retains the legacy system-state fields at the JSON root.
    #[serde(flatten)]
    pub state: T,
    /// Declares support authenticated by the signature over the whole body.
    #[serde(default)]
    pub capabilities: AgentCapabilities,
    /// Reports only previously supplied guest targets, signed with the parent state.
    /// Missing reports preserve compatibility and do not imply a guest outage.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub guest_observations: Vec<GuestObservation>,
}

/// Identifies one explicitly associated external guest for read-only observation.
/// The unit is always `microvm@<guest_identity>.service`, never a command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuestObservationTarget {
    /// Identifies the registered guest resource, independent of its configuration.
    pub guest_id: uuid::Uuid,
    /// Names the guest instance on the parent host (the registered hostname).
    pub guest_identity: String,
    /// Names the exact MicroVM systemd service to inspect.
    pub unit: String,
}

impl GuestObservationTarget {
    /// Returns whether the identity is safe and the unit is its exact service.
    /// Escapes, patterns, paths, whitespace and arbitrary service names are forbidden.
    ///
    /// # Examples
    /// ```
    /// use cf_protocol::agent::GuestObservationTarget;
    /// let target = GuestObservationTarget {
    ///     guest_id: uuid::Uuid::new_v4(),
    ///     guest_identity: "vm-photos".into(),
    ///     unit: "microvm@vm-photos.service".into(),
    /// };
    /// assert!(target.is_valid());
    /// ```
    pub fn is_valid(&self) -> bool {
        let identity = self.guest_identity.as_bytes();
        !identity.is_empty()
            && identity[0].is_ascii_alphanumeric()
            && identity
                .iter()
                .all(|c| c.is_ascii_alphanumeric() || b"-_.".contains(c))
            && self.unit.len() <= 255
            && self.unit == format!("microvm@{}.service", self.guest_identity)
    }
}

/// Retains parent-reported unit evidence, never guest attestation or scan authority.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuestObservation {
    /// Binds evidence to the exact server-provided guest identity and unit.
    #[serde(flatten)]
    pub target: GuestObservationTarget,
    /// Contains the unit's ActiveState, or None when collection was unavailable.
    pub unit_active_state: Option<String>,
    /// Identifies a locally proven runner store object, not a guest NixOS closure.
    /// None means the runner could not be established from an authoritative path.
    pub runner_store_path: Option<String>,
    /// Records parent collection time; servers reject future observations.
    pub observed_at: DateTime<Utc>,
}

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

    #[test]
    fn guest_targets_reject_commands_patterns_and_mismatched_units() {
        let mut target = GuestObservationTarget {
            guest_id: uuid::Uuid::new_v4(),
            guest_identity: "vm-photos".into(),
            unit: "microvm@vm-photos.service".into(),
        };
        assert!(target.is_valid());
        target.unit = "sshd.service".into();
        assert!(!target.is_valid());
        for identity in [
            "",
            "--help",
            "guest*",
            "../guest",
            "guest;reboot",
            "guest\n",
            "guest@other",
            "guest\\x20",
        ] {
            target.guest_identity = identity.into();
            target.unit = format!("microvm@{identity}.service");
            assert!(!target.is_valid(), "{identity:?}");
        }
        target.guest_identity = "a".repeat(256);
        target.unit = format!("microvm@{}.service", target.guest_identity);
        assert!(!target.is_valid());
    }

    #[test]
    fn guest_observation_fields_are_additive_and_legacy_defaults_are_empty() {
        let legacy = serde_json::json!({"hostname":"parent", "change_reason":"heartbeat"});
        let request: CurrentSystemRequest<SystemState> =
            serde_json::from_value(legacy.clone()).unwrap();
        assert!(request.guest_observations.is_empty());
        let response: LogResponse =
            serde_json::from_value(serde_json::json!({"desired_target":null})).unwrap();
        assert!(response.guest_observation_targets.is_empty());
        let report = GuestObservation {
            target: GuestObservationTarget {
                guest_id: uuid::Uuid::new_v4(),
                guest_identity: "guest".into(),
                unit: "microvm@guest.service".into(),
            },
            unit_active_state: None,
            runner_store_path: None,
            observed_at: Utc::now(),
        };
        let current = CurrentSystemRequest {
            state: request.state,
            capabilities: AgentCapabilities::default(),
            guest_observations: vec![report],
        };
        let json = serde_json::to_value(current).unwrap();
        assert_eq!(json["guest_observations"][0]["guest_identity"], "guest");
        assert!(json["guest_observations"][0]["runner_store_path"].is_null());
        let old: SystemState = serde_json::from_value(json.clone()).unwrap();
        assert_eq!(old.hostname, "parent");
        let new: CurrentSystemRequest<SystemState> = serde_json::from_value(json).unwrap();
        assert_eq!(new.guest_observations.len(), 1);
    }

    #[test]
    fn basic_read_capability_is_independent_and_defaults_false() {
        for legacy in [
            serde_json::json!({}),
            serde_json::json!({"supports_niks3":true}),
        ] {
            let caps: AgentCapabilities = serde_json::from_value(legacy).unwrap();
            assert!(!caps.supports_niks3_basic_read);
        }
        let request: CurrentSystemRequest<serde_json::Value> =
            serde_json::from_value(serde_json::json!({"hostname":"host", "capabilities":{
                "supports_niks3":true, "supports_niks3_basic_read":true
            }}))
            .unwrap();
        assert!(request.capabilities.supports_niks3_basic_read);
        assert!(request.capabilities.supports_niks3);
        assert_eq!(
            serde_json::to_value(request).unwrap()["capabilities"]["supports_niks3_basic_read"],
            true
        );
    }

    #[test]
    fn legacy_current_system_json_defaults_missing_capabilities_to_false() {
        let legacy = serde_json::json!({
            "hostname": "legacy-host",
            "change_reason": "startup",
            "agent_version": "999.0.0",
            "agent_build_hash": "claimed-current",
            "network_interfaces": "[]"
        });
        let parsed: CurrentSystemRequest<SystemState> =
            serde_json::from_value(legacy.clone()).unwrap();
        assert_eq!(parsed.capabilities, AgentCapabilities::default());
        let old: SystemState = serde_json::from_value(legacy.clone()).unwrap();
        assert_eq!(
            serde_json::to_value(&parsed.state).unwrap(),
            serde_json::to_value(&old).unwrap()
        );
        for capabilities in [
            serde_json::json!({}),
            serde_json::json!({"supports_niks3": false}),
        ] {
            let mut body = legacy.clone();
            body["capabilities"] = capabilities;
            let request: CurrentSystemRequest<SystemState> = serde_json::from_value(body).unwrap();
            assert!(!request.capabilities.supports_niks3);
        }
    }

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
        assert!(value.get("capabilities").is_none());
        let legacy: CurrentSystemRequest<SystemState> =
            serde_json::from_value(value.clone()).unwrap();
        assert!(!legacy.capabilities.supports_niks3);
        let advertised = serde_json::to_value(CurrentSystemRequest {
            state: &state,
            guest_observations: Vec::new(),
            capabilities: AgentCapabilities {
                supports_niks3: true,
                ..AgentCapabilities::default()
            },
        })
        .unwrap();
        assert_eq!(advertised["capabilities"]["supports_niks3"], true);
        // Old parsers ignore the additive field and retain the original state.
        let old_parser: SystemState = serde_json::from_value(advertised).unwrap();
        assert_eq!(serde_json::to_value(old_parser).unwrap(), value);
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
    /// Signing keys for the read endpoint; legacy keys remain available below.
    #[serde(default)]
    pub cache_public_keys: Vec<String>,
    /// Read-only credentials scoped to the receiving agent's environment.
    #[serde(default)]
    pub read_auth: crate::cache::CacheReadAuth,
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
    /// Replaces the parent's target list for its next normal pull heartbeat.
    /// An absent list revokes cached targets; this never requests synchronous refresh.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub guest_observation_targets: Vec<GuestObservationTarget>,
}
