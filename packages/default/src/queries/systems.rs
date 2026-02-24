use crate::models::systems::System;
use anyhow::Result;
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use std::collections::BTreeSet;
use uuid::Uuid;

pub async fn update_hostname(pool: &PgPool, system: &System, new_hostname: &str) -> Result<()> {
    sqlx::query("UPDATE systems SET hostname = $1, updated_at = NOW() WHERE id = $2")
        .bind(new_hostname)
        .bind(system.id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn get_by_hostname(pool: &PgPool, hostname: &str) -> Result<Option<System>> {
    let system = sqlx::query_as::<_, System>("SELECT * FROM systems WHERE hostname = $1")
        .bind(hostname)
        .fetch_optional(pool)
        .await?;
    Ok(system)
}

pub async fn get_by_id(pool: &PgPool, id: i32) -> Result<Option<System>> {
    let system = sqlx::query_as::<_, System>("SELECT * FROM systems WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await?;
    Ok(system)
}

pub async fn insert_system(pool: &PgPool, system: &System) -> Result<System> {
    let inserted = sqlx::query_as::<_, System>(
        r#"
    INSERT INTO systems (
        hostname,
        environment_id,
        is_active,
        public_key,
        flake_id,
        derivation,
        created_at,
        updated_at,
        desired_target,
        deployment_policy
    )
    VALUES ($1, $2, $3, $4, $5, $6, NOW(), NOW(), $7, $8)
    ON CONFLICT (hostname) DO UPDATE SET
        environment_id = EXCLUDED.environment_id,
        is_active = EXCLUDED.is_active,
        public_key = EXCLUDED.public_key,
        flake_id = EXCLUDED.flake_id,
        derivation = EXCLUDED.derivation,
        desired_target = EXCLUDED.desired_target,
        deployment_policy = EXCLUDED.deployment_policy,
        updated_at = NOW()
    RETURNING *
    "#,
    )
    .bind(&system.hostname)
    .bind(system.environment_id)
    .bind(system.is_active)
    .bind(&system.public_key.to_base64())
    .bind(system.flake_id)
    .bind(&system.derivation)
    .bind(&system.desired_target)
    .bind(&system.deployment_policy)
    .fetch_one(pool)
    .await?;
    Ok(inserted)
}

pub async fn get_desired_target_by_hostname(
    pool: &PgPool,
    hostname: &str,
) -> Result<Option<String>> {
    let result = sqlx::query_scalar::<_, Option<String>>(
        "SELECT desired_target FROM systems WHERE hostname = $1",
    )
    .bind(hostname)
    .fetch_optional(pool)
    .await?;

    // Handle the nested Option from fetch_optional + nullable column
    Ok(result.flatten())
}

pub async fn get_desired_target_by_id(pool: &PgPool, system_id: i32) -> Result<Option<String>> {
    let result =
        sqlx::query_scalar::<_, Option<String>>("SELECT desired_target FROM systems WHERE id = $1")
            .bind(system_id)
            .fetch_optional(pool)
            .await?;

    // Handle the nested Option from fetch_optional + nullable column
    Ok(result.flatten())
}

#[derive(Debug, sqlx::FromRow)]
pub struct SystemAccessRow {
    pub id: Uuid,
    pub hostname: String,
    pub environment_id: Option<Uuid>,
    pub environment: Option<String>,
    pub is_active: bool,
    pub deployment_policy: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub async fn list_system_access_rows(pool: &PgPool) -> Result<Vec<SystemAccessRow>> {
    let rows = sqlx::query_as::<_, SystemAccessRow>(
        "SELECT s.id,
                s.hostname,
                s.environment_id,
                e.name AS environment,
                s.is_active,
                s.deployment_policy,
                s.created_at,
                s.updated_at
         FROM systems s
         LEFT JOIN environments e ON e.id = s.environment_id",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn find_system_access_row(
    pool: &PgPool,
    system_id: Uuid,
) -> Result<Option<SystemAccessRow>> {
    let row = sqlx::query_as::<_, SystemAccessRow>(
        "SELECT s.id,
                s.hostname,
                s.environment_id,
                e.name AS environment,
                s.is_active,
                s.deployment_policy,
                s.created_at,
                s.updated_at
         FROM systems s
         LEFT JOIN environments e ON e.id = s.environment_id
         WHERE s.id = $1",
    )
    .bind(system_id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

pub async fn touch_system_updated_at(pool: &PgPool, system_id: Uuid) -> Result<()> {
    sqlx::query("UPDATE systems SET updated_at = NOW() WHERE id = $1")
        .bind(system_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn update_system_desired_target(
    pool: &PgPool,
    system_id: Uuid,
    target_commit: &str,
) -> Result<()> {
    sqlx::query("UPDATE systems SET desired_target = $1, updated_at = NOW() WHERE id = $2")
        .bind(target_commit)
        .bind(system_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn get_user_environment_membership_ids(
    pool: &PgPool,
    user_id: Uuid,
) -> Result<BTreeSet<Uuid>> {
    let ids = sqlx::query_scalar::<_, Uuid>(
        "SELECT environment_id FROM user_environment_memberships WHERE user_id = $1",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    Ok(ids.into_iter().collect())
}

/// Row type for view_system_detail
#[derive(Debug, sqlx::FromRow)]
pub struct SystemDetailRow {
    pub id: Uuid,
    pub hostname: String,
    pub environment: Option<String>,
    pub is_active: bool,
    pub deployment_policy: String,
    pub health_status: String,
    pub deployment_status: String,
    pub pipeline_stage: String,
    pub nixos_version: Option<String>,
    pub kernel: Option<String>,
    pub agent_version: Option<String>,
    pub current_store_path: Option<String>,
    // Hardware
    pub cpu_brand: Option<String>,
    pub cpu_cores: Option<i32>,
    pub memory_gb: Option<f64>,
    pub uptime_secs: Option<i64>,
    pub board_serial: Option<String>,
    pub bios_version: Option<String>,
    // Network
    pub primary_ip_address: Option<String>,
    pub primary_mac_address: Option<String>,
    pub gateway_ip: Option<String>,
    // Security
    pub tpm_present: Option<bool>,
    pub secure_boot_enabled: Option<bool>,
    pub fips_mode: Option<bool>,
    pub selinux_status: Option<String>,
    // Hardware change flags
    pub hardware_changed_24h: Option<bool>,
    pub hardware_ever_changed: Option<bool>,
    // CVE counts
    pub critical_cve_count: i32,
    pub high_cve_count: i32,
    pub medium_cve_count: i32,
    pub low_cve_count: i32,
    // Flake info
    pub flake_id: Option<i32>,
    pub flake_name: Option<String>,
    pub flake_repo_url: Option<String>,
    pub flake_latest_commit: Option<String>,
    // Timestamps
    pub last_seen: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Fetch system detail from view_system_detail
pub async fn get_system_detail_by_id(
    pool: &PgPool,
    system_id: Uuid,
) -> Result<Option<SystemDetailRow>> {
    let row = sqlx::query_as::<_, SystemDetailRow>(
        "SELECT * FROM view_system_detail WHERE id = $1",
    )
    .bind(system_id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// Row type for view_system_list
#[derive(Debug, sqlx::FromRow)]
pub struct SystemListRow {
    pub id: Uuid,
    pub hostname: String,
    pub environment: Option<String>,
    pub primary_ip_address: Option<String>,
    pub health_status: String,
    pub deployment_status: String,
    pub pipeline_stage: String,
    pub critical_cve_count: i32,
    pub high_cve_count: i32,
    pub medium_cve_count: i32,
    pub low_cve_count: i32,
    pub nixos_version: Option<String>,
    pub last_seen: Option<DateTime<Utc>>,
    pub deployment_policy: String,
}

/// Fetch all active systems from view_system_list
pub async fn list_systems_from_view(pool: &PgPool) -> Result<Vec<SystemListRow>> {
    let rows = sqlx::query_as::<_, SystemListRow>(
        "SELECT * FROM view_system_list ORDER BY hostname",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}
