//! Crystal Forge server configuration.
//!
//! This module re-exports all configuration types from `cf-config` and adds
//! server-specific functionality: database pool creation, connection validation,
//! and configuration synchronization to the database.
//!
//! # Summary of what lives where
//!
//! - Pure config loading, structs, defaults → `cf-config`
//! - Database pool creation, validation, and DB sync → this module (server-only)

// Re-export everything from cf-config at the same path so existing imports
// (e.g., `use crate::config::CrystalForgeConfig`) continue to work.
pub use cf_config::config::*;

// Re-export sub-modules that callers reference explicitly
// (e.g., `use crate::config::deployment::DeploymentConfig`)
pub use cf_config::config::deployment;

use crate::models::systems::System;
use crate::queries::environments::{
    get_environment_id_by_name, get_or_insert_environment_id_by_config,
};
use crate::queries::flakes::{get_flake_id_by_repo_url, insert_flake};
use crate::queries::systems::insert_system;
use anyhow::{Context, Result};
use sqlx::{PgPool, postgres::PgPoolOptions};
use std::env;
use std::time::Duration;
use tokio_postgres::NoTls;

// ─────────────────────────────────────────────────────────────────────────────
// Server-side database operations for CrystalForgeConfig
//
// These functions require SQLx/PostgreSQL and therefore cannot live in cf-config.
// Call them from the server binary after loading CrystalForgeConfig::load().
// ─────────────────────────────────────────────────────────────────────────────

/// Create and return a PostgreSQL connection pool from the loaded config.
pub async fn db_pool_from_config(cfg: &CrystalForgeConfig) -> Result<PgPool> {
    let db_url = cfg.database.to_url();
    PgPoolOptions::new()
        .max_connections(20)
        .min_connections(5)
        .acquire_timeout(Duration::from_secs(30))
        .idle_timeout(Some(Duration::from_secs(600)))
        .max_lifetime(Some(Duration::from_secs(1800)))
        .test_before_acquire(true)
        .connect(&db_url)
        .await
        .context("connecting to database")
}

/// Convenience wrapper: loads config then creates a DB pool.
///
/// This mirrors the old `CrystalForgeConfig::db_pool()` associated function.
/// Server code that needs a quick pool without an existing config can call this.
pub async fn db_pool() -> Result<PgPool> {
    let cfg = CrystalForgeConfig::load()?;
    db_pool_from_config(&cfg).await
}

/// Validate that a database connection can be established using the config.
pub async fn validate_db_connection() -> anyhow::Result<()> {
    let cfg = CrystalForgeConfig::load()?;
    let db_url = cfg.database.to_url();
    let (_client, connection) = tokio_postgres::connect(&db_url, NoTls).await?;
    tokio::spawn(connection);
    Ok(())
}

/// Synchronize static systems from TOML config into the database.
///
/// Upserts all systems declared in `config.toml` into the systems table,
/// creating environment and flake records as needed.
pub async fn sync_systems_to_db(cfg: &CrystalForgeConfig, pool: &PgPool) -> anyhow::Result<()> {
    let reloaded = CrystalForgeConfig::load()?;

    if cfg.systems.is_empty() {
        let config_path = env::var("CRYSTAL_FORGE_CONFIG")
            .unwrap_or_else(|_| "/var/lib/crystal_forge/config.toml".to_string());
        tracing::info!(
            "No systems defined in {}; skipping system sync.",
            config_path
        );
        return Ok(());
    }

    tracing::debug!("💡 Syncing Systems in Config to Database.");

    // Sync environments first
    for environment in &reloaded.environments {
        let _ = get_or_insert_environment_id_by_config(pool, environment).await?;
    }

    for config in &cfg.systems {
        tracing::info!("📥 Syncing system {}...", config.hostname);

        let environment_id = get_environment_id_by_name(pool, &config.environment)
            .await?
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "Environment '{}' not found in database for system '{}'",
                    config.environment,
                    config.hostname
                )
            })?;

        let flake_id = if let Some(flake_name) = &config.flake_name {
            let watched_flake = cfg
                .flakes
                .watched
                .iter()
                .find(|wf| &wf.name == flake_name)
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "Flake '{}' referenced by system '{}' not found in flakes.watched",
                        flake_name,
                        config.hostname
                    )
                })?;

            let id = match get_flake_id_by_repo_url(pool, &watched_flake.repo_url).await? {
                Some(id) => id,
                None => {
                    insert_flake(
                        pool,
                        &watched_flake.name,
                        &watched_flake.repo_url,
                        &watched_flake.branch(),
                        "cf_systems_only",
                    )
                    .await?
                    .id
                }
            };
            Some(id)
        } else {
            None
        };

        let system = System::new(
            pool,
            config.hostname.clone(),
            Some(environment_id),
            true,
            config.public_key.clone(),
            flake_id,
            None,
            config.desired_target.clone(),
            config.deployment_policy.clone(),
        )
        .await?;
        insert_system(pool, &system).await;
    }

    Ok(())
}
