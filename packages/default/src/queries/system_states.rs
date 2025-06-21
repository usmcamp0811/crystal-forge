use crate::models::systems::SystemState;
use anyhow::{Context, Result};
use sqlx::{PgPool, Row};

pub async fn insert_system_state(pool: &PgPool, state: &SystemState) -> Result<()> {
    sqlx::query(
        r#"INSERT INTO tbl_system_states (
            hostname, 
            derivation_path,
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
    .bind(&state.derivation_path) // required
    .bind(&state.context)
    .bind(&state.os) // Option<String>
    .bind(&state.kernel) // Option<String>
    .bind(state.memory_gb) // Option<f64>
    .bind(state.uptime_secs) // Option<i64>
    .bind(&state.cpu_brand) // Option<String>
    .bind(state.cpu_cores) // Option<i32>
    .bind(&state.board_serial) // Option<String>
    .bind(&state.product_uuid) // Option<String>
    .bind(&state.rootfs_uuid) // Option<String>
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
