use crate::config;
use anyhow::{Context, Result};
use sqlx::{PgPool, postgres::PgPoolOptions};

pub async fn get_db_client() -> Result<PgPool> {
    let db_config = config::load_config()?;
    let db_url = db_config
        .database
        .as_ref()
        .context("missing [database] section in config")?
        .to_url();

    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&db_url)
        .await
        .context("failed to connect to database")?;

    Ok(pool)
}
