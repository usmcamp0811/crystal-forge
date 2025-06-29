use crate::handlers::current_system::try_deserialize_system_state;
use crate::models::system_states::SystemState;
use anyhow::{Context, Result};
use sqlx::{PgPool, Row};

/// Fetch the environment record associated with this system
pub async fn get_environment(&self, pool: &PgPool) -> Result<Option<Environment>> {
    let env = sqlx::query_as::<_, Environment>("SELECT * FROM environment WHERE id = $1")
        .bind(self.environment_id)
        .fetch_optional(pool)
        .await?;
    Ok(env)
}
