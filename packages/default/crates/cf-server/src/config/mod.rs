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

use crate::models::public_key::PublicKey;
use crate::models::systems::{ManagementMode, System};
use crate::queries::environments::{
    get_environment_id_by_name, get_or_insert_environment_id_by_config,
};
use crate::queries::flakes::{get_flake_id_by_repo_url, insert_flake};
use crate::queries::systems::SystemReconciliationModeConflict;
use anyhow::{Context, Result};
use sqlx::{PgPool, postgres::PgPoolOptions};
use std::env;
use std::time::Duration;
use tokio_postgres::NoTls;

#[cfg(test)]
mod database_url_tests;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::public_key::PublicKey;
    use crate::queries::systems::{insert_system, update_management_authority_tx};
    use ed25519_dalek::SigningKey;

    fn registration(hostname: &str) -> SystemConfig {
        let key = SigningKey::from_bytes(&[42; 32]);
        SystemConfig {
            hostname: hostname.into(),
            public_key: PublicKey::from_verifying_key(key.verifying_key()).to_base64(),
            environment: "reconciliation".into(),
            flake_name: None,
            deployment_policy: "manual".into(),
            desired_target: None,
        }
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires an isolated PostgreSQL database with CREATEDB; owner runs database regressions"]
    async fn management_mode_config_sync_preserves_converted_registrations(pool: PgPool) {
        let mut cfg = CrystalForgeConfig::default();
        for mode in [ManagementMode::External, ManagementMode::Reference] {
            let mut config = registration(&format!("converted-{mode}"));
            let stale_system = System::new(
                &pool,
                config.hostname.clone(),
                None,
                true,
                config.public_key.clone(),
                None,
                None,
                Some("/nix/store/legacy-target".into()),
                "manual".into(),
            )
            .await
            .unwrap();
            sqlx::query(
                "UPDATE systems SET tags=ARRAY['retained'], description='operator notes',
                 agent_enrolled_at='2026-01-01T00:00:00Z' WHERE id=$1",
            )
            .bind(stale_system.id)
            .execute(&pool)
            .await
            .unwrap();
            sqlx::query(
                "INSERT INTO pending_system_deployments(system_id,target_store_path)
                 VALUES($1,'/nix/store/legacy-target')",
            )
            .bind(stale_system.id)
            .execute(&pool)
            .await
            .unwrap();
            sqlx::query(
                "INSERT INTO system_states(hostname,store_path,change_reason)
                 VALUES($1,'/nix/store/historical-target','cf_deployment')",
            )
            .bind(&config.hostname)
            .execute(&pool)
            .await
            .unwrap();
            let mut tx = pool.begin().await.unwrap();
            sqlx::query("SELECT id FROM systems WHERE id=$1 FOR UPDATE")
                .bind(stale_system.id)
                .execute(&mut *tx)
                .await
                .unwrap();
            update_management_authority_tx(&mut tx, stale_system.id, mode, None, None)
                .await
                .unwrap();
            tx.commit().await.unwrap();

            // A stale managed value must still fail at the final SQL guard,
            // including when conversion occurs after reconciliation's read.
            let error = insert_system(&pool, &stale_system).await.unwrap_err();
            assert!(error.is::<SystemReconciliationModeConflict>());
            let snapshot_sql = "SELECT jsonb_build_object(
                'system', to_jsonb(s),
                'pending', (SELECT jsonb_agg(to_jsonb(p) ORDER BY p.id)
                            FROM pending_system_deployments p WHERE p.system_id=s.id),
                'history', (SELECT jsonb_agg(to_jsonb(h) ORDER BY h.id)
                            FROM system_states h WHERE h.hostname=s.hostname))
                FROM systems s WHERE s.id=$1";
            let before: serde_json::Value = sqlx::query_scalar(snapshot_sql)
                .bind(stale_system.id)
                .fetch_one(&pool)
                .await
                .unwrap();
            assert_eq!(before["system"]["management_mode"], mode.as_str());
            assert!(before["system"]["public_key"].is_null());
            assert!(before["system"]["desired_target"].is_null());
            assert_eq!(before["pending"][0]["status"], "superseded");
            assert_eq!(before["history"].as_array().unwrap().len(), 1);

            // Each obsolete field would fail if resolved for this resource.
            config.environment = "removed-environment".into();
            config.flake_name = Some("removed-flake".into());
            config.public_key = "obsolete-invalid-key".into();
            config.deployment_policy = "obsolete-invalid-policy".into();
            config.desired_target = Some("/nix/store/stale-config-target".into());
            cfg.systems = vec![config.clone()];
            sync_systems_to_db(&cfg, &pool).await.unwrap();
            // Exercise the skip independently of missing-environment failure.
            sqlx::query("INSERT INTO environments(name) VALUES($1) ON CONFLICT DO NOTHING")
                .bind(&config.environment)
                .execute(&pool)
                .await
                .unwrap();
            sync_systems_to_db(&cfg, &pool).await.unwrap();
            cfg.systems[0].flake_name = None;
            sync_systems_to_db(&cfg, &pool).await.unwrap();
            let after: serde_json::Value = sqlx::query_scalar(snapshot_sql)
                .bind(stale_system.id)
                .fetch_one(&pool)
                .await
                .unwrap();
            assert_eq!(
                after, before,
                "startup must preserve the entire registration and history"
            );
        }
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires an isolated PostgreSQL database with CREATEDB; owner runs database regressions"]
    async fn management_mode_config_sync_preserves_fresh_enrollment(pool: PgPool) {
        sqlx::query("INSERT INTO environments(name) VALUES('reconciliation')")
            .execute(&pool)
            .await
            .unwrap();
        for mode in [ManagementMode::External, ManagementMode::Reference] {
            let mut cfg = CrystalForgeConfig::default();
            cfg.systems = vec![registration(&format!("reenrolled-{mode}"))];
            sync_systems_to_db(&cfg, &pool).await.unwrap();
            let obsolete: System = sqlx::query_as("SELECT * FROM systems WHERE hostname=$1")
                .bind(&cfg.systems[0].hostname)
                .fetch_one(&pool)
                .await
                .unwrap();
            sqlx::query(
                "UPDATE systems SET tags=ARRAY['retained'], description='operator notes',
                 agent_enrolled_at='2026-01-01T00:00:00Z' WHERE id=$1",
            )
            .bind(obsolete.id)
            .execute(&pool)
            .await
            .unwrap();
            sqlx::query(
                "INSERT INTO system_states(hostname,store_path,change_reason)
                 VALUES($1,'/nix/store/historical-target','cf_deployment')",
            )
            .bind(&obsolete.hostname)
            .execute(&pool)
            .await
            .unwrap();
            sqlx::query(
                "INSERT INTO pending_system_deployments(system_id,target_store_path)
                 VALUES($1,'/nix/store/obsolete-target')",
            )
            .bind(obsolete.id)
            .execute(&pool)
            .await
            .unwrap();
            let fresh_key =
                PublicKey::from_verifying_key(SigningKey::from_bytes(&[43; 32]).verifying_key())
                    .to_base64();
            let mut tx = pool.begin().await.unwrap();
            sqlx::query("SELECT id FROM systems WHERE id=$1 FOR UPDATE")
                .bind(obsolete.id)
                .execute(&mut *tx)
                .await
                .unwrap();
            update_management_authority_tx(&mut tx, obsolete.id, mode, None, None)
                .await
                .unwrap();
            update_management_authority_tx(
                &mut tx,
                obsolete.id,
                ManagementMode::Managed,
                None,
                Some(&fresh_key),
            )
            .await
            .unwrap();
            tx.commit().await.unwrap();

            let snapshot_sql = "SELECT jsonb_build_object(
                'system', to_jsonb(s),
                'pending', (SELECT jsonb_agg(to_jsonb(p) ORDER BY p.id)
                            FROM pending_system_deployments p WHERE p.system_id=s.id),
                'history', (SELECT jsonb_agg(to_jsonb(h) ORDER BY h.id)
                            FROM system_states h WHERE h.hostname=s.hostname),
                'revoked', (SELECT jsonb_agg(to_jsonb(r) ORDER BY r.public_key)
                            FROM system_revoked_agent_keys r WHERE r.system_id=s.id))
                FROM systems s WHERE s.id=$1";
            let before: serde_json::Value = sqlx::query_scalar(snapshot_sql)
                .bind(obsolete.id)
                .fetch_one(&pool)
                .await
                .unwrap();
            assert_eq!(before["system"]["public_key"], fresh_key);
            assert_eq!(before["system"]["management_mode"], "managed");
            assert_eq!(before["system"]["deployment_policy"], "manual");
            assert!(!before["system"]["agent_enrolled_at"].is_null());
            assert_eq!(before["pending"][0]["status"], "superseded");
            assert_eq!(before["history"].as_array().unwrap().len(), 1);

            // Neither a well-formed obsolete key nor missing/malformed keys
            // authorize resolving removed legacy fields or restoring automation.
            let valid = cfg.systems[0].clone();
            for key in [&valid.public_key, "", "obsolete-invalid-key"] {
                cfg.systems[0] = valid.clone();
                cfg.systems[0].public_key = key.into();
                cfg.systems[0].environment = "removed-environment".into();
                cfg.systems[0].flake_name = Some("removed-flake".into());
                cfg.systems[0].deployment_policy = "obsolete-invalid-policy".into();
                cfg.systems[0].desired_target = Some("/nix/store/obsolete-target".into());
                sync_systems_to_db(&cfg, &pool).await.unwrap();
                cfg.systems[0].environment = valid.environment.clone();
                sync_systems_to_db(&cfg, &pool).await.unwrap();
                cfg.systems[0].flake_name = None;
                sync_systems_to_db(&cfg, &pool).await.unwrap();
                cfg.systems[0].deployment_policy = "auto_latest".into();
                sync_systems_to_db(&cfg, &pool).await.unwrap();
            }
            let error = insert_system(&pool, &obsolete).await.unwrap_err();
            assert!(error.is::<SystemReconciliationModeConflict>());
            let after: serde_json::Value = sqlx::query_scalar(snapshot_sql)
                .bind(obsolete.id)
                .fetch_one(&pool)
                .await
                .unwrap();
            assert_eq!(
                after, before,
                "stale config must preserve all persisted state"
            );

            cfg.systems[0] = valid;
            cfg.systems[0].public_key = fresh_key;
            cfg.systems[0].deployment_policy = "auto_latest".into();
            sync_systems_to_db(&cfg, &pool).await.unwrap();
            let mut reconciled: serde_json::Value = sqlx::query_scalar(snapshot_sql)
                .bind(obsolete.id)
                .fetch_one(&pool)
                .await
                .unwrap();
            assert_eq!(reconciled["system"]["deployment_policy"], "auto_latest");
            // Only the requested policy and reconciliation timestamp may change.
            reconciled["system"]["deployment_policy"] =
                before["system"]["deployment_policy"].clone();
            reconciled["system"]["updated_at"] = before["system"]["updated_at"].clone();
            assert_eq!(reconciled, before);
        }
    }

    /// Observes a real row-lock wait so the upsert must check the newly committed
    /// enrollment, rather than relying on the pre-conversion visible row.
    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires an isolated PostgreSQL database with CREATEDB; owner runs database regressions"]
    async fn management_mode_obsolete_upsert_waiting_on_conversion_is_rejected(pool: PgPool) {
        let config = registration("config-conversion-race");
        let obsolete = System::new(
            &pool,
            config.hostname,
            None,
            true,
            config.public_key,
            None,
            None,
            None,
            "auto_latest".into(),
        )
        .await
        .unwrap();
        let mut tx = pool.begin().await.unwrap();
        let owner: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
            .fetch_one(&mut *tx)
            .await
            .unwrap();
        sqlx::query("SELECT id FROM systems WHERE id=$1 FOR UPDATE")
            .bind(obsolete.id)
            .execute(&mut *tx)
            .await
            .unwrap();
        update_management_authority_tx(&mut tx, obsolete.id, ManagementMode::Reference, None, None)
            .await
            .unwrap();
        let fresh_key =
            PublicKey::from_verifying_key(SigningKey::from_bytes(&[43; 32]).verifying_key())
                .to_base64();
        update_management_authority_tx(
            &mut tx,
            obsolete.id,
            ManagementMode::Managed,
            None,
            Some(&fresh_key),
        )
        .await
        .unwrap();
        sqlx::query("UPDATE systems SET deployment_policy='manual' WHERE id=$1")
            .bind(obsolete.id)
            .execute(&mut *tx)
            .await
            .unwrap();
        let snapshot_sql = "SELECT to_jsonb(s) FROM systems s WHERE id=$1";
        let before: serde_json::Value = sqlx::query_scalar(snapshot_sql)
            .bind(obsolete.id)
            .fetch_one(&mut *tx)
            .await
            .unwrap();
        let id = obsolete.id;
        let writer_pool = pool.clone();
        let writer = tokio::spawn(async move { insert_system(&writer_pool, &obsolete).await });
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                let blocked: bool = sqlx::query_scalar(
                    "SELECT EXISTS (SELECT 1 FROM pg_stat_activity
                     WHERE $1 = ANY(pg_blocking_pids(pid))
                       AND query LIKE '%INSERT INTO systems%')",
                )
                .bind(owner)
                .fetch_one(&pool)
                .await
                .unwrap();
                if blocked {
                    break;
                }
                assert!(!writer.is_finished(), "upsert must wait for conversion");
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("upsert must reach the system-row lock");
        tx.commit().await.unwrap();
        assert!(
            writer
                .await
                .unwrap()
                .unwrap_err()
                .is::<SystemReconciliationModeConflict>()
        );
        let after: serde_json::Value = sqlx::query_scalar(snapshot_sql)
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(after, before);
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires an isolated PostgreSQL database with CREATEDB; owner runs database regressions"]
    async fn management_mode_config_sync_retains_managed_behavior_and_errors(pool: PgPool) {
        sqlx::query("INSERT INTO environments(name) VALUES('reconciliation')")
            .execute(&pool)
            .await
            .unwrap();
        let mut cfg = CrystalForgeConfig::default();
        cfg.systems = vec![registration("still-managed")];
        sync_systems_to_db(&cfg, &pool).await.unwrap();
        sqlx::query(
            "UPDATE systems SET tags=ARRAY['managed-tag'], description='managed notes',
             agent_enrolled_at='2026-01-01T00:00:00Z' WHERE hostname='still-managed'",
        )
        .execute(&pool)
        .await
        .unwrap();
        let metadata_sql = "SELECT jsonb_build_array(id,tags,description,agent_enrolled_at)
                            FROM systems WHERE hostname='still-managed'";
        let before: serde_json::Value = sqlx::query_scalar(metadata_sql)
            .fetch_one(&pool)
            .await
            .unwrap();
        cfg.systems[0].deployment_policy = "auto_latest".into();
        sync_systems_to_db(&cfg, &pool).await.unwrap();
        let after: serde_json::Value = sqlx::query_scalar(metadata_sql)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(after, before);
        let stored: (String, String, Option<String>) = sqlx::query_as(
            "SELECT management_mode,deployment_policy,public_key FROM systems WHERE hostname='still-managed'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            stored,
            (
                "managed".into(),
                "auto_latest".into(),
                Some(cfg.systems[0].public_key.clone())
            )
        );

        let valid = cfg.systems[0].clone();
        for invalid_field in ["environment", "flake", "policy"] {
            cfg.systems[0] = valid.clone();
            match invalid_field {
                "environment" => cfg.systems[0].environment = "missing".into(),
                "flake" => cfg.systems[0].flake_name = Some("missing".into()),
                "policy" => cfg.systems[0].deployment_policy = "invalid".into(),
                _ => unreachable!(),
            }
            let error = sync_systems_to_db(&cfg, &pool).await.unwrap_err();
            assert!(!error.is::<SystemReconciliationModeConflict>());
        }

        // Before enrollment, invalid keys remain errors and valid declarative
        // key changes remain supported.
        sqlx::query("UPDATE systems SET agent_enrolled_at=NULL WHERE hostname='still-managed'")
            .execute(&pool)
            .await
            .unwrap();
        cfg.systems[0] = valid;
        cfg.systems[0].public_key = "invalid".into();
        assert!(sync_systems_to_db(&cfg, &pool).await.is_err());
        cfg.systems[0].public_key =
            PublicKey::from_verifying_key(SigningKey::from_bytes(&[43; 32]).verifying_key())
                .to_base64();
        sync_systems_to_db(&cfg, &pool).await.unwrap();
        let stored: String =
            sqlx::query_scalar("SELECT public_key FROM systems WHERE hostname='still-managed'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(stored, cfg.systems[0].public_key);
    }
}

/// Synchronizes static managed systems from TOML config into the database.
///
/// Existing nonmanaged registrations take precedence over legacy system config.
/// Skips them before resolving environment, flake, key, or policy fields. A
/// concurrent authority change rejected by the final SQL guard is also skipped.
/// Enrolled managed registrations accept config only when its validated key
/// matches the persisted canonical key, before resolving other legacy fields.
/// Managed registrations without an enrollment epoch retain declarative key
/// reconciliation. Operator metadata and enrollment remain database-owned.
///
/// # Errors
/// Returns configuration loading, validation, and database errors. Only
/// [`SystemReconciliationModeConflict`] is treated as a skipped registration.
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
        // SECURITY: Revoked authority takes precedence over stale legacy fields.
        // Keep the final upsert guard for conversion after this read.
        let existing = sqlx::query_as::<
            _,
            (uuid::Uuid, ManagementMode, Option<chrono::DateTime<chrono::Utc>>, Option<String>),
        >(
            "SELECT id, management_mode, agent_enrolled_at, public_key FROM systems WHERE hostname=$1",
        )
        .bind(&config.hostname)
        .fetch_optional(pool)
        .await?;
        if let Some((resource_id, mode, enrolled_at, public_key)) = existing {
            if !mode.permits_deployment() {
                tracing::warn!(%resource_id, "Skipping legacy system config for a nonmanaged registration");
                continue;
            }
            if enrolled_at.is_some() {
                let configured_key = PublicKey::from_base64(&config.public_key, "")
                    .ok()
                    .map(|key| key.to_base64());
                if configured_key.is_none() || configured_key != public_key {
                    tracing::warn!(%resource_id, "Skipping legacy system config due to an enrollment authority conflict");
                    continue;
                }
            }
        }

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

        let result = System::new(
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
        .await;
        match result {
            Ok(_) => {}
            Err(error) if error.is::<SystemReconciliationModeConflict>() => {
                let resource_id: uuid::Uuid =
                    sqlx::query_scalar("SELECT id FROM systems WHERE hostname=$1")
                        .bind(&config.hostname)
                        .fetch_one(pool)
                        .await?;
                tracing::warn!(%resource_id, "Skipping legacy system config after a mode or enrollment authority conflict");
            }
            Err(error) => return Err(error),
        }
    }

    Ok(())
}
