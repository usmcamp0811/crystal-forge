use crate::models::system_states::SystemState;
use crate::queries::system_states::{
    get_last_system_state_by_hostname, get_latest_system_state_id,
};

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use sqlx::PgPool;

#[derive(Debug, FromRow, Serialize, Deserialize)]
pub struct AgentHeartbeat {
    pub id: i64,
    pub system_state_id: i32,
    pub timestamp: DateTime<Utc>,
    pub agent_version: Option<String>,
    pub agent_build_hash: Option<String>,
}

impl AgentHeartbeat {
    /// Creates a heartbeat from SystemState if it's a heartbeat and state hasn't changed
    /// Returns Ok(heartbeat) if this should be recorded as heartbeat only
    /// Returns Err if this represents a state change that needs full logging
    pub async fn from_system_state_if_heartbeat(
        state: &SystemState,
        pool: &PgPool,
    ) -> Result<Self, StateChangeRequired> {
        // First check if this is a heartbeat-type change reason
        if !Self::is_heartbeat_change_reason(&state.change_reason) {
            return Err(StateChangeRequired::NotHeartbeatType);
        }

        // Get the last known state for this system
        let last_state = match get_last_system_state_by_hostname(pool, &state.hostname).await {
            Ok(Some(state)) => state,
            Ok(None) => {
                // No previous state exists - this is the first report
                return Err(StateChangeRequired::FirstReport);
            }
            Err(_db_error) => {
                // Database error - treat as requiring state change to be safe
                return Err(StateChangeRequired::DatabaseError);
            }
        };

        // Compare states to see if anything meaningful changed
        if Self::states_are_equivalent(state, &last_state) {
            // States are the same - create heartbeat
            Ok(Self {
                id: 0, // Will be set by database
                system_state_id: match get_latest_system_state_id(pool, &state.hostname).await {
                    Ok(Some(id)) => id,
                    Ok(None) => return Err(StateChangeRequired::FirstReport),
                    Err(_) => return Err(StateChangeRequired::DatabaseError),
                },
                timestamp: state.timestamp.unwrap_or_else(|| Utc::now()),
                agent_version: state.agent_version.clone(),
                agent_build_hash: state.agent_build_hash.clone(),
            })
        } else {
            // State changed - needs full update
            Err(StateChangeRequired::StateChanged)
        }
    }

    /// Check if the change reason indicates this should be a heartbeat
    fn is_heartbeat_change_reason(change_reason: &str) -> bool {
        matches!(change_reason, "heartbeat")
    }

    /// Compare two system states to determine if they're equivalent
    /// Ignores timestamp, uptime, and other expected-to-change fields
    fn states_are_equivalent(current: &SystemState, previous: &SystemState) -> bool {
        // Compare all fields except those that naturally change over time
        current.hostname == previous.hostname
            && current.derivation_path == previous.derivation_path
            && current.os == previous.os
            && current.kernel == previous.kernel
            && current.memory_gb == previous.memory_gb
            && current.cpu_brand == previous.cpu_brand
            && current.cpu_cores == previous.cpu_cores
            && current.board_serial == previous.board_serial
            && current.product_uuid == previous.product_uuid
            && current.rootfs_uuid == previous.rootfs_uuid
            && current.chassis_serial == previous.chassis_serial
            && current.bios_version == previous.bios_version
            && current.cpu_microcode == previous.cpu_microcode
            && current.network_interfaces == previous.network_interfaces
            && current.primary_mac_address == previous.primary_mac_address
            && current.primary_ip_address == previous.primary_ip_address
            && current.gateway_ip == previous.gateway_ip
            && current.selinux_status == previous.selinux_status
            && current.tpm_present == previous.tpm_present
            && current.secure_boot_enabled == previous.secure_boot_enabled
            && current.fips_mode == previous.fips_mode
            && current.agent_version == previous.agent_version
            && current.agent_build_hash == previous.agent_build_hash
            && current.nixos_version == previous.nixos_version
        // Note: Deliberately ignoring uptime_secs and timestamp as they always change
    }
}

#[derive(Debug)]
pub enum StateChangeRequired {
    NotHeartbeatType,
    FirstReport,
    StateChanged,
    DatabaseError, // Add this variant
}

impl std::fmt::Display for StateChangeRequired {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotHeartbeatType => write!(f, "Change reason indicates this is not a heartbeat"),
            Self::FirstReport => write!(f, "No previous state exists - first report"),
            Self::StateChanged => write!(f, "System state has changed from previous report"),
            Self::DatabaseError => write!(f, "Database error occurred while checking state"),
        }
    }
}

impl std::error::Error for StateChangeRequired {}
