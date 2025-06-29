use crate::handlers::current_system::try_deserialize_system_state;
use crate::models::system_states::SystemState;
use anyhow::{Context, Result};
use sqlx::{PgPool, Row};

/// Update the hostname of this system
pub async fn update_hostname(&self, pool: &PgPool, new_hostname: &str) -> Result<()> {
    sqlx::query("UPDATE tbl_systems SET hostname = $1, updated_at = NOW() WHERE id = $2")
        .bind(new_hostname)
        .bind(self.id)
        .execute(pool)
        .await?;
    Ok(())
}
