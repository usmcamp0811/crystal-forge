use crate::models::agent_heartbeats::AgentHeartbeat;
use anyhow::Result;

/// Insert an agent heartbeat row.
///
/// Accepts any Postgres executor so callers can run it against a pool or
/// inside a transaction (e.g. atomically with the boot_id update).
pub async fn insert_agent_heartbeat<'e, E>(executor: E, heartbeat: &AgentHeartbeat) -> Result<()>
where
    E: sqlx::PgExecutor<'e>,
{
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
    .execute(executor)
    .await?;

    Ok(())
}
