use anyhow::{Context, Result};
use sqlx::{PgPool, Row};

pub async fn insert_system_state(pool: &PgPool, state: &SystemState) -> Result<()> {
    sqlx::query(
        r#"INSERT INTO tbl_system_states (
            hostname, 
            system_derivation_id,
            context, 
            os, 
            kernel,
            memory_gb, 
            uptime_secs, 
            cpu_brand, 
            cpu_cores,
            board_serial, 
            product_uuid, 
            rootfs_uuid
        ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)
        ON CONFLICT DO NOTHING"#,
    )
    .bind(&state.hostname)
    .bind(&state.system_derivation_id)
    .bind(&state.context)
    .bind(&state.os)
    .bind(&state.kernel)
    .bind(state.memory_gb)
    .bind(state.uptime_secs)
    .bind(&state.cpu_brand)
    .bind(state.cpu_cores)
    .bind(&state.board_serial)
    .bind(&state.product_uuid)
    .bind(&state.rootfs_uuid)
    .execute(pool)
    .await
    .context("failed to insert system state")?;

    Ok(())
}
