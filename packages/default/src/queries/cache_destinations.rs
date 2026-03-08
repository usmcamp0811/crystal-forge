use crate::models::cache_destination::{CacheDestination, CreateCacheDestination, UpdateCacheDestination};
use anyhow::Result;
use sqlx::PgPool;
use tracing::debug;

/// List all cache destinations, optionally filtering by enabled status
pub async fn list_cache_destinations(
    pool: &PgPool,
    enabled_only: bool,
) -> Result<Vec<CacheDestination>> {
    let sql = if enabled_only {
        "SELECT * FROM cache_destinations WHERE enabled = true ORDER BY name"
    } else {
        "SELECT * FROM cache_destinations ORDER BY name"
    };

    let destinations = sqlx::query_as::<_, CacheDestination>(sql)
        .fetch_all(pool)
        .await?;

    debug!("Listed {} cache destinations", destinations.len());
    Ok(destinations)
}

/// Get a single cache destination by ID
pub async fn get_cache_destination(pool: &PgPool, id: i32) -> Result<Option<CacheDestination>> {
    let destination = sqlx::query_as::<_, CacheDestination>(
        "SELECT * FROM cache_destinations WHERE id = $1"
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;

    Ok(destination)
}

/// Get a single cache destination by name
pub async fn get_cache_destination_by_name(
    pool: &PgPool,
    name: &str,
) -> Result<Option<CacheDestination>> {
    let destination = sqlx::query_as::<_, CacheDestination>(
        "SELECT * FROM cache_destinations WHERE name = $1"
    )
    .bind(name)
    .fetch_optional(pool)
    .await?;

    Ok(destination)
}

/// Create a new cache destination
pub async fn create_cache_destination(
    pool: &PgPool,
    create: &CreateCacheDestination,
) -> Result<CacheDestination> {
    // Validate before inserting
    create.validate().map_err(|e| anyhow::anyhow!(e))?;

    let destination = sqlx::query_as::<_, CacheDestination>(
        r#"
        INSERT INTO cache_destinations (
            name, cache_type, push_to, enabled, signing_key_path, compression,
            s3_region, s3_profile, attic_token, attic_cache_name, 
            attic_ignore_upstream_cache_filter, attic_jobs,
            parallel_uploads, max_retries, retry_delay_seconds, push_timeout_seconds,
            force_repush, require_sigs
        ) VALUES (
            $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18
        )
        RETURNING *
        "#,
    )
    .bind(&create.name)
    .bind(&create.cache_type)
    .bind(&create.push_to)
    .bind(create.enabled.unwrap_or(true))
    .bind(&create.signing_key_path)
    .bind(&create.compression)
    .bind(&create.s3_region)
    .bind(&create.s3_profile)
    .bind(&create.attic_token)
    .bind(&create.attic_cache_name)
    .bind(create.attic_ignore_upstream_cache_filter)
    .bind(create.attic_jobs)
    .bind(create.parallel_uploads)
    .bind(create.max_retries)
    .bind(create.retry_delay_seconds)
    .bind(create.push_timeout_seconds)
    .bind(create.force_repush)
    .bind(create.require_sigs)
    .fetch_one(pool)
    .await?;

    debug!("Created cache destination: {}", destination.name);
    Ok(destination)
}

/// Update an existing cache destination
pub async fn update_cache_destination(
    pool: &PgPool,
    id: i32,
    update: &UpdateCacheDestination,
) -> Result<Option<CacheDestination>> {
    // Build dynamic update query based on which fields are provided
    let mut query = String::from("UPDATE cache_destinations SET ");
    let mut updates = Vec::new();
    let mut bind_count = 1;

    if let Some(ref name) = update.name {
        if name.trim().is_empty() {
            return Err(anyhow::anyhow!("Cache destination name cannot be empty"));
        }
        updates.push(format!("name = ${}", bind_count));
        bind_count += 1;
    }
    if let Some(ref cache_type) = update.cache_type {
        if !matches!(cache_type.as_str(), "S3" | "Attic" | "Http" | "Nix") {
            return Err(anyhow::anyhow!("Invalid cache_type: {}", cache_type));
        }
        updates.push(format!("cache_type = ${}", bind_count));
        bind_count += 1;
    }
    if update.push_to.is_some() {
        updates.push(format!("push_to = ${}", bind_count));
        bind_count += 1;
    }
    if update.enabled.is_some() {
        updates.push(format!("enabled = ${}", bind_count));
        bind_count += 1;
    }
    if update.signing_key_path.is_some() {
        updates.push(format!("signing_key_path = ${}", bind_count));
        bind_count += 1;
    }
    if update.compression.is_some() {
        updates.push(format!("compression = ${}", bind_count));
        bind_count += 1;
    }
    if update.s3_region.is_some() {
        updates.push(format!("s3_region = ${}", bind_count));
        bind_count += 1;
    }
    if update.s3_profile.is_some() {
        updates.push(format!("s3_profile = ${}", bind_count));
        bind_count += 1;
    }
    if update.attic_token.is_some() {
        updates.push(format!("attic_token = ${}", bind_count));
        bind_count += 1;
    }
    if update.attic_cache_name.is_some() {
        updates.push(format!("attic_cache_name = ${}", bind_count));
        bind_count += 1;
    }
    if update.attic_ignore_upstream_cache_filter.is_some() {
        updates.push(format!("attic_ignore_upstream_cache_filter = ${}", bind_count));
        bind_count += 1;
    }
    if update.attic_jobs.is_some() {
        updates.push(format!("attic_jobs = ${}", bind_count));
        bind_count += 1;
    }
    if update.parallel_uploads.is_some() {
        updates.push(format!("parallel_uploads = ${}", bind_count));
        bind_count += 1;
    }
    if update.max_retries.is_some() {
        updates.push(format!("max_retries = ${}", bind_count));
        bind_count += 1;
    }
    if update.retry_delay_seconds.is_some() {
        updates.push(format!("retry_delay_seconds = ${}", bind_count));
        bind_count += 1;
    }
    if update.push_timeout_seconds.is_some() {
        updates.push(format!("push_timeout_seconds = ${}", bind_count));
        bind_count += 1;
    }
    if update.force_repush.is_some() {
        updates.push(format!("force_repush = ${}", bind_count));
        bind_count += 1;
    }
    if update.require_sigs.is_some() {
        updates.push(format!("require_sigs = ${}", bind_count));
        bind_count += 1;
    }

    if updates.is_empty() {
        // No fields to update, just return the existing record
        return get_cache_destination(pool, id).await;
    }

    query.push_str(&updates.join(", "));
    query.push_str(&format!(" WHERE id = ${} RETURNING *", bind_count));

    let mut q = sqlx::query_as::<_, CacheDestination>(&query);

    // Bind values in the same order as the updates
    if let Some(ref name) = update.name {
        q = q.bind(name);
    }
    if let Some(ref cache_type) = update.cache_type {
        q = q.bind(cache_type);
    }
    if let Some(ref push_to) = update.push_to {
        q = q.bind(push_to);
    }
    if let Some(enabled) = update.enabled {
        q = q.bind(enabled);
    }
    if let Some(ref signing_key_path) = update.signing_key_path {
        q = q.bind(signing_key_path);
    }
    if let Some(ref compression) = update.compression {
        q = q.bind(compression);
    }
    if let Some(ref s3_region) = update.s3_region {
        q = q.bind(s3_region);
    }
    if let Some(ref s3_profile) = update.s3_profile {
        q = q.bind(s3_profile);
    }
    if let Some(ref attic_token) = update.attic_token {
        q = q.bind(attic_token);
    }
    if let Some(ref attic_cache_name) = update.attic_cache_name {
        q = q.bind(attic_cache_name);
    }
    if let Some(attic_ignore_upstream_cache_filter) = update.attic_ignore_upstream_cache_filter {
        q = q.bind(attic_ignore_upstream_cache_filter);
    }
    if let Some(attic_jobs) = update.attic_jobs {
        q = q.bind(attic_jobs);
    }
    if let Some(parallel_uploads) = update.parallel_uploads {
        q = q.bind(parallel_uploads);
    }
    if let Some(max_retries) = update.max_retries {
        q = q.bind(max_retries);
    }
    if let Some(retry_delay_seconds) = update.retry_delay_seconds {
        q = q.bind(retry_delay_seconds);
    }
    if let Some(push_timeout_seconds) = update.push_timeout_seconds {
        q = q.bind(push_timeout_seconds);
    }
    if let Some(force_repush) = update.force_repush {
        q = q.bind(force_repush);
    }
    if let Some(require_sigs) = update.require_sigs {
        q = q.bind(require_sigs);
    }

    // Bind the ID for WHERE clause
    q = q.bind(id);

    let destination = q.fetch_optional(pool).await?;

    if let Some(ref dest) = destination {
        debug!("Updated cache destination: {}", dest.name);
    }

    Ok(destination)
}

/// Delete a cache destination
pub async fn delete_cache_destination(pool: &PgPool, id: i32) -> Result<bool> {
    let result = sqlx::query("DELETE FROM cache_destinations WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;

    let deleted = result.rows_affected() > 0;
    if deleted {
        debug!("Deleted cache destination with id: {}", id);
    }

    Ok(deleted)
}

/// Update the last_used_at timestamp for a cache destination
pub async fn update_cache_destination_last_used(pool: &PgPool, name: &str) -> Result<()> {
    sqlx::query(
        "UPDATE cache_destinations SET last_used_at = NOW() WHERE name = $1"
    )
    .bind(name)
    .execute(pool)
    .await?;

    debug!("Updated last_used_at for cache destination: {}", name);
    Ok(())
}
