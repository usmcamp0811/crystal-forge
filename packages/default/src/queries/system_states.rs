use crate::models::systems::SystemState;
use anyhow::{Context, Result};
use sqlx::{PgPool, Row};

pub async fn insert_system_state(pool: &PgPool, state: &SystemState) -> Result<()> {
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
            systemd_version
        ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21,$22,$23,$24,$25,$26,$27)
        ON CONFLICT DO NOTHING"#,
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
    .bind(&state.systemd_version)
    .execute(pool)
    .await
    .map_err(|e| anyhow::anyhow!("SQL error: {e:?}"))
    .with_context(|| {
        format!(
            "failed to insert system state for host={} context={} hash={}",
            state.hostname,
            state.context,
            state.derivation_path.as_deref().unwrap_or("none")
        )
    })?;
    Ok(())
}
