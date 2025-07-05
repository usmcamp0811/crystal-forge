use crate::models::commits::Commit;
use anyhow::{Context, Result};
use sqlx::PgPool;

pub async fn insert_agent_heartbeat(pool: &PgPool, heartbeat: &AgentHeartbeat) -> Result<()> {
    sqlx::query!(
        r#"
       INSERT INTO agent_heartbeats (system_state_id, timestamp, agent_version, agent_build_hash)
       VALUES ($1, $2, $3, $4)
       "#,
        heartbeat.system_state_id,
        heartbeat.timestamp,
        heartbeat.agent_version,
        heartbeat.agent_build_hash
    )
    .execute(pool)
    .await?;

    Ok(())
}
