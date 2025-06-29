use crate::models::system_states::SystemState;
use anyhow::Result;
use sqlx::PgPool;

pub async fn insert_system_state(
    pool: &PgPool,
    state: &SystemState,
    version_compatible: bool,
) -> Result<()> {
    sqlx::query(
        r#"INSERT INTO tbl_system_states (
            hostname, 
            context,
            derivation_path,
            os, 
            kernel,
            memory_gb, 
            uptime_secs, 
            cpu_brand, 
            cpu_cores,
            board_serial, 
            product_uuid, 
            rootfs_uuid,
            chassis_serial,
            bios_version,
            cpu_microcode,
            network_interfaces,
            primary_mac_address,
            primary_ip_address,
            gateway_ip,
            selinux_status,
            tpm_present,
            secure_boot_enabled,
            fips_mode,
            agent_version,
            agent_build_hash,
            nixos_version,
            agent_compatible,
            partial_data
        ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21,$22,$23,$24,$25,$26,$27,$28)"#,
    )
    .bind(&state.hostname)
    .bind(&state.context)
    .bind(&state.derivation_path)
    .bind(&state.os)
    .bind(&state.kernel)
    .bind(state.memory_gb)
    .bind(state.uptime_secs)
    .bind(&state.cpu_brand)
    .bind(state.cpu_cores)
    .bind(&state.board_serial)
    .bind(&state.product_uuid)
    .bind(&state.rootfs_uuid)
    .bind(&state.chassis_serial)
    .bind(&state.bios_version)
    .bind(&state.cpu_microcode)
    .bind(&state.network_interfaces)
    .bind(&state.primary_mac_address)
    .bind(&state.primary_ip_address)
    .bind(&state.gateway_ip)
    .bind(&state.selinux_status)
    .bind(state.tpm_present)
    .bind(state.secure_boot_enabled)
    .bind(state.fips_mode)
    .bind(&state.agent_version)
    .bind(&state.agent_build_hash)
    .bind(&state.nixos_version)
    .bind(version_compatible)  // $27
    .bind(!version_compatible) // $28 - partial_data flag
    .execute(pool)
    .await
    .map_err(|e| anyhow::anyhow!("SQL error: {e:?}"))?;

    // Optionally log incompatible agents for monitoring
    if !version_compatible {
        tracing::warn!(
            "Agent version incompatibility detected: host={} version={} - agent should be upgraded",
            state.hostname,
            state.agent_version.as_deref().unwrap_or("unknown")
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handlers::current_system::try_deserialize_system_state;
    use crate::models::system_states::{SystemState, SystemStateV1};
    use anyhow::Result;

    #[test]
    fn test_try_deserialize_current_version() {
        let current_state = SystemState {
            id: None,
            hostname: "test-host".to_string(),
            context: "test-context".to_string(),
            derivation_path: Some("/nix/store/test".to_string()),
            os: Some("NixOS".to_string()),
            kernel: Some("6.1.0".to_string()),
            memory_gb: Some(16.0),
            uptime_secs: Some(3600),
            cpu_brand: Some("Test CPU".to_string()),
            cpu_cores: Some(8),
            board_serial: Some("TEST123".to_string()),
            product_uuid: Some("test-uuid".to_string()),
            rootfs_uuid: Some("root-uuid".to_string()),
            timestamp: None,
            chassis_serial: Some("chassis-123".to_string()),
            bios_version: Some("1.0".to_string()),
            cpu_microcode: Some("microcode-1".to_string()),
            network_interfaces: Some(
                serde_json::json!([{"name":"eth0","mac":"00:11:22:33:44:55"}]),
            ),
            primary_mac_address: Some("00:11:22:33:44:55".to_string()),
            primary_ip_address: Some("192.168.1.100".to_string()),
            gateway_ip: Some("192.168.1.1".to_string()),
            selinux_status: Some("disabled".to_string()),
            tpm_present: Some(true),
            secure_boot_enabled: Some(false),
            fips_mode: Some(false),
            agent_version: Some("1.0.0".to_string()),
            agent_build_hash: Some("abc123".to_string()),
            nixos_version: Some("23.05".to_string()),
        };

        let json = serde_json::to_vec(&current_state).unwrap();
        let (parsed, compatible) = try_deserialize_system_state(&json).unwrap();

        assert!(compatible, "Current version should be compatible");
        assert_eq!(parsed.hostname, "test-host");
        assert_eq!(parsed.agent_version, Some("1.0.0".to_string()));
    }

    #[test]
    fn test_try_deserialize_v1_fallback() {
        // For now, since V1 and current have the same fields, this test
        // will actually succeed as compatible. We'll update this when
        // we have actual breaking changes between versions.
        let v1_state = SystemStateV1 {
            id: None,
            hostname: "test-host-v1".to_string(),
            context: "test-context".to_string(),
            derivation_path: Some("/nix/store/test".to_string()),
            os: Some("NixOS".to_string()),
            kernel: Some("6.1.0".to_string()),
            memory_gb: Some(16.0),
            uptime_secs: Some(3600),
            cpu_brand: Some("Test CPU".to_string()),
            cpu_cores: Some(8),
            board_serial: Some("TEST123".to_string()),
            product_uuid: Some("test-uuid".to_string()),
            rootfs_uuid: Some("root-uuid".to_string()),
            timestamp: None,
        };

        let json = serde_json::to_vec(&v1_state).unwrap();
        let (parsed, compatible) = try_deserialize_system_state(&json).unwrap();

        // Currently this will be compatible since V1 fields are a subset of current
        // assert!(compatible, "V1 JSON should currently be compatible");
        assert_eq!(parsed.hostname, "test-host-v1");
        // New fields should be None since they weren't in the JSON
        assert_eq!(parsed.agent_version, None);
        assert_eq!(parsed.nixos_version, None);
    }

    #[test]
    fn test_invalid_json_fails() {
        let invalid_json = b"{\"invalid\": \"json\", \"missing\": \"fields\"}";
        let result = try_deserialize_system_state(invalid_json);
        assert!(result.is_err(), "Invalid JSON should fail to deserialize");
    }

    #[test]
    fn test_system_state_from_v1_conversion() {
        let v1 = SystemStateV1 {
            id: Some(1),
            hostname: "test".to_string(),
            context: "ctx".to_string(),
            derivation_path: None,
            os: Some("NixOS".to_string()),
            kernel: Some("6.1".to_string()),
            memory_gb: Some(8.0),
            uptime_secs: Some(1000),
            cpu_brand: Some("Intel".to_string()),
            cpu_cores: Some(4),
            board_serial: None,
            product_uuid: None,
            rootfs_uuid: None,
            timestamp: None,
        };

        let current = SystemState::from_v1(v1);

        assert_eq!(current.hostname, "test");
        assert_eq!(current.context, "ctx");
        assert_eq!(current.os, Some("NixOS".to_string()));
        assert_eq!(current.agent_version, None); // New field should be None
        assert_eq!(current.nixos_version, None); // New field should be None
        assert_eq!(current.chassis_serial, None); // New field should be None
    }
}
