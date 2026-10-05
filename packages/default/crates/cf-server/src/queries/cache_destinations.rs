use crate::models::cache_destination::{
    CacheDestination, CreateCacheDestination, UpdateCacheDestination, effective_cache_url,
};
use crate::security::cache_secrets;
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

    let destinations = destinations
        .into_iter()
        .map(decrypt_destination_secrets)
        .collect::<Result<Vec<_>>>()?;

    debug!("Listed {} cache destinations", destinations.len());
    Ok(destinations)
}

/// Get a single cache destination by ID
pub async fn get_cache_destination(pool: &PgPool, id: i32) -> Result<Option<CacheDestination>> {
    let destination =
        sqlx::query_as::<_, CacheDestination>("SELECT * FROM cache_destinations WHERE id = $1")
            .bind(id)
            .fetch_optional(pool)
            .await?;

    destination.map(decrypt_destination_secrets).transpose()
}

/// Get a single cache destination by name
pub async fn get_cache_destination_by_name(
    pool: &PgPool,
    name: &str,
) -> Result<Option<CacheDestination>> {
    let destination =
        sqlx::query_as::<_, CacheDestination>("SELECT * FROM cache_destinations WHERE name = $1")
            .bind(name)
            .fetch_optional(pool)
            .await?;

    destination.map(decrypt_destination_secrets).transpose()
}

fn decrypt_destination_secrets(mut destination: CacheDestination) -> Result<CacheDestination> {
    // SECURITY: Legacy or directly written certificate fields can contain private
    // PEM material. Fail closed before returning plaintext fields to API callers.
    destination
        .validate_niks3_certificates()
        .map_err(anyhow::Error::msg)?;
    destination.attic_token = cache_secrets::decrypt_optional(destination.attic_token.as_deref())?;
    destination.s3_access_key_id =
        cache_secrets::decrypt_optional(destination.s3_access_key_id.as_deref())?;
    destination.s3_secret_access_key =
        cache_secrets::decrypt_optional(destination.s3_secret_access_key.as_deref())?;
    destination.s3_session_token =
        cache_secrets::decrypt_optional(destination.s3_session_token.as_deref())?;
    destination.niks3_auth_token =
        cache_secrets::decrypt_optional(destination.niks3_auth_token.as_deref())?;
    destination.niks3_write_client_key =
        cache_secrets::decrypt_optional(destination.niks3_write_client_key.as_deref())?;
    destination.niks3_read_client_key =
        cache_secrets::decrypt_optional(destination.niks3_read_client_key.as_deref())?;
    destination.refresh_niks3_configured();
    Ok(destination)
}

/// Encrypts legacy plaintext cache credentials and returns the updated row count.
///
/// # Errors
/// Returns an error if encryption or database access fails.
pub async fn encrypt_plaintext_cache_secrets(pool: &PgPool) -> Result<u64> {
    // CONCURRENCY: Lock rows until encryption commits so startup backfill cannot
    // overwrite a concurrent credential replacement with an older secret.
    let mut tx = pool.begin().await?;
    let destinations =
        sqlx::query_as::<_, CacheDestination>("SELECT * FROM cache_destinations FOR UPDATE")
            .fetch_all(&mut *tx)
            .await?;

    let mut updated: u64 = 0;
    for destination in destinations {
        let attic = destination.attic_token.as_deref();
        let s3_access = destination.s3_access_key_id.as_deref();
        let s3_secret = destination.s3_secret_access_key.as_deref();
        let s3_session = destination.s3_session_token.as_deref();
        let niks3_token = destination.niks3_auth_token.as_deref();
        let niks3_write_key = destination.niks3_write_client_key.as_deref();
        let niks3_read_key = destination.niks3_read_client_key.as_deref();

        let needs_update = attic.is_some_and(|v| !cache_secrets::is_encrypted(v))
            || s3_access.is_some_and(|v| !cache_secrets::is_encrypted(v))
            || s3_secret.is_some_and(|v| !cache_secrets::is_encrypted(v))
            || s3_session.is_some_and(|v| !cache_secrets::is_encrypted(v))
            || niks3_token.is_some_and(|v| !cache_secrets::is_encrypted(v))
            || niks3_write_key.is_some_and(|v| !cache_secrets::is_encrypted(v))
            || niks3_read_key.is_some_and(|v| !cache_secrets::is_encrypted(v));

        if !needs_update {
            continue;
        }

        let encrypted_attic = cache_secrets::encrypt_optional(attic)?;
        let encrypted_s3_access = cache_secrets::encrypt_optional(s3_access)?;
        let encrypted_s3_secret = cache_secrets::encrypt_optional(s3_secret)?;
        let encrypted_s3_session = cache_secrets::encrypt_optional(s3_session)?;

        sqlx::query(
            "UPDATE cache_destinations
             SET attic_token = $2,
                 s3_access_key_id = $3,
                 s3_secret_access_key = $4,
                  s3_session_token = $5,
                  niks3_auth_token = $6,
                  niks3_write_client_key = $7,
                  niks3_read_client_key = $8
             WHERE id = $1",
        )
        .bind(destination.id)
        .bind(encrypted_attic)
        .bind(encrypted_s3_access)
        .bind(encrypted_s3_secret)
        .bind(encrypted_s3_session)
        .bind(cache_secrets::encrypt_optional(niks3_token)?)
        .bind(cache_secrets::encrypt_optional(niks3_write_key)?)
        .bind(cache_secrets::encrypt_optional(niks3_read_key)?)
        .execute(&mut *tx)
        .await?;

        updated += 1;
    }

    tx.commit().await?;
    Ok(updated)
}

/// Creates a validated cache destination and its environment assignments atomically.
///
/// # Errors
/// Returns an error for invalid configuration, encryption failure, or DB failure.
pub async fn create_cache_destination(
    pool: &PgPool,
    create: &CreateCacheDestination,
) -> Result<CacheDestination> {
    // Validate before inserting
    create.validate().map_err(|e| anyhow::anyhow!(e))?;

    // Start transaction
    let mut tx = pool.begin().await?;

    let encrypted_s3_access_key_id =
        cache_secrets::encrypt_optional(create.s3_access_key_id.as_deref())?;
    let encrypted_s3_secret_access_key =
        cache_secrets::encrypt_optional(create.s3_secret_access_key.as_deref())?;
    let encrypted_s3_session_token =
        cache_secrets::encrypt_optional(create.s3_session_token.as_deref())?;
    let encrypted_attic_token = cache_secrets::encrypt_optional(create.attic_token.as_deref())?;

    let mut destination = sqlx::query_as::<_, CacheDestination>(
        r#"
        INSERT INTO cache_destinations (
            name, cache_type, push_to, enabled, signing_key_path, compression,
            s3_region, s3_profile, s3_access_key_id, s3_secret_access_key, s3_session_token, s3_endpoint_url,
            attic_token, attic_cache_name, attic_public_key,
            attic_ignore_upstream_cache_filter, attic_jobs,
            parallel_uploads, max_retries, retry_delay_seconds, push_timeout_seconds,
            force_repush, require_sigs,
            niks3_server_url, niks3_public_keys, niks3_write_auth_mode, niks3_auth_token,
            niks3_write_client_cert, niks3_write_client_key, niks3_write_ca_cert,
            niks3_read_auth_mode, niks3_read_client_cert, niks3_read_client_key, niks3_read_ca_cert
        ) VALUES (
            $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21, $22, $23,
            $24, $25, $26, $27, $28, $29, $30, $31, $32, $33, $34
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
    .bind(&encrypted_s3_access_key_id)
    .bind(&encrypted_s3_secret_access_key)
    .bind(&encrypted_s3_session_token)
    .bind(&create.s3_endpoint_url)
    .bind(&encrypted_attic_token)
    .bind(&create.attic_cache_name)
    .bind(&create.attic_public_key)
    .bind(create.attic_ignore_upstream_cache_filter)
    .bind(create.attic_jobs)
    .bind(create.parallel_uploads)
    .bind(create.max_retries)
    .bind(create.retry_delay_seconds)
    .bind(create.push_timeout_seconds)
    .bind(create.force_repush)
    .bind(create.require_sigs)
    .bind(&create.niks3_server_url)
    .bind(&create.niks3_public_keys)
    .bind(&create.niks3_write_auth_mode)
    .bind(cache_secrets::encrypt_optional(create.niks3_auth_token.as_deref())?)
    .bind(&create.niks3_write_client_cert)
    .bind(cache_secrets::encrypt_optional(create.niks3_write_client_key.as_deref())?)
    .bind(&create.niks3_write_ca_cert)
    .bind(&create.niks3_read_auth_mode)
    .bind(&create.niks3_read_client_cert)
    .bind(cache_secrets::encrypt_optional(create.niks3_read_client_key.as_deref())?)
    .bind(&create.niks3_read_ca_cert)
    .fetch_one(&mut *tx)
    .await?;

    // Assign environments if provided
    if let Some(ref env_ids) = create.environment_ids {
        destination.updated_at =
            replace_cache_environments_tx(&mut tx, destination.id, env_ids).await?;
    }

    tx.commit().await?;

    let destination = decrypt_destination_secrets(destination)?;

    debug!(
        "Created cache destination: {} with {} environment assignments",
        destination.name,
        create
            .environment_ids
            .as_ref()
            .map(|e| e.len())
            .unwrap_or(0)
    );
    Ok(destination)
}

/// Updates a destination and assignments atomically, preserving omitted secrets.
///
/// Authentication mode transitions clear the old mode's credentials before
/// validation. Concurrent updates serialize on the destination row lock.
/// URL writes use [`effective_update`], preserving same-type sanitized round
/// trips and removing inherited URI authentication on type conversions.
///
/// # Errors
/// Returns an error for invalid resulting settings, encryption, or DB failure.
pub async fn update_cache_destination(
    pool: &PgPool,
    id: i32,
    update: &UpdateCacheDestination,
) -> Result<Option<CacheDestination>> {
    // CONCURRENCY: Hold the row lock across read, merge, validation, credential
    // replacement, and assignment updates to prevent lost credential changes.
    let mut tx = pool.begin().await?;
    let current = sqlx::query_as::<_, CacheDestination>(
        "SELECT * FROM cache_destinations WHERE id = $1 FOR UPDATE",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(current) = current.map(decrypt_destination_secrets).transpose()? else {
        return Ok(None);
    };

    let niks3 = effective_update(&current, update)
        .map_err(|_| anyhow::anyhow!("Invalid effective cache configuration"))?;
    let type_changed = niks3.cache_type != current.cache_type;
    let update_push_to = update.push_to.is_some() || type_changed;
    let update_s3_endpoint = update.s3_endpoint_url.is_some() || type_changed;
    // Preserve stored ciphertext on unrelated updates. Write all Niks3 columns
    // together only when that configuration or its cache type changes.
    let update_niks3 = (current.cache_type == "Niks3"
        && update.cache_type.as_deref().is_some_and(|ty| ty != "Niks3"))
        || update.cache_type.as_deref() == Some("Niks3")
        || update.niks3_server_url.is_some()
        || !update.niks3_public_keys.is_empty()
        || update.niks3_write_auth_mode.is_some()
        || update.niks3_auth_token.is_some()
        || update.niks3_write_client_cert.is_some()
        || update.niks3_write_client_key.is_some()
        || update.niks3_write_ca_cert.is_some()
        || update.niks3_read_auth_mode.is_some()
        || update.niks3_read_client_cert.is_some()
        || update.niks3_read_client_key.is_some()
        || update.niks3_read_ca_cert.is_some()
        || update.clear_niks3_auth_token
        || update.clear_niks3_write_client_key
        || update.clear_niks3_read_client_key
        || update.clear_niks3_write_ca_cert
        || update.clear_niks3_read_ca_cert;

    // Build dynamic update query based on which fields are provided
    let mut query = String::from("UPDATE cache_destinations SET ");
    let mut updates = Vec::new();
    let mut bind_count = 1;

    if update
        .cache_type
        .as_deref()
        .is_some_and(|ty| ty != current.cache_type)
    {
        // SECURITY: Save must not activate stale inactive credentials that the
        // shared effective merge intentionally excluded from validation/Test.
        for (column, replaced) in [
            ("attic_token", update.attic_token.is_some()),
            ("s3_access_key_id", update.s3_access_key_id.is_some()),
            (
                "s3_secret_access_key",
                update.s3_secret_access_key.is_some(),
            ),
            ("s3_session_token", update.s3_session_token.is_some()),
        ] {
            if !replaced {
                updates.push(format!("{column} = NULL"));
            }
        }
    }

    if let Some(ref name) = update.name {
        if name.trim().is_empty() {
            return Err(anyhow::anyhow!("Cache destination name cannot be empty"));
        }
        updates.push(format!("name = ${}", bind_count));
        bind_count += 1;
    }
    if let Some(ref cache_type) = update.cache_type {
        if !matches!(
            cache_type.as_str(),
            "S3" | "Attic" | "Http" | "Nix" | "Niks3"
        ) {
            return Err(anyhow::anyhow!("Invalid cache_type: {}", cache_type));
        }
        updates.push(format!("cache_type = ${}", bind_count));
        bind_count += 1;
    }
    if update_push_to {
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
    if update.s3_access_key_id.is_some() {
        updates.push(format!("s3_access_key_id = ${}", bind_count));
        bind_count += 1;
    }
    if update.s3_secret_access_key.is_some() {
        updates.push(format!("s3_secret_access_key = ${}", bind_count));
        bind_count += 1;
    }
    if update.s3_session_token.is_some() {
        updates.push(format!("s3_session_token = ${}", bind_count));
        bind_count += 1;
    }
    if update_s3_endpoint {
        updates.push(format!("s3_endpoint_url = ${}", bind_count));
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
    if update.attic_public_key.is_some() {
        updates.push(format!("attic_public_key = ${}", bind_count));
        bind_count += 1;
    }
    if update.attic_ignore_upstream_cache_filter.is_some() {
        updates.push(format!(
            "attic_ignore_upstream_cache_filter = ${}",
            bind_count
        ));
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

    if update_niks3 {
        for column in [
            "niks3_server_url",
            "niks3_public_keys",
            "niks3_write_auth_mode",
            "niks3_auth_token",
            "niks3_write_client_cert",
            "niks3_write_client_key",
            "niks3_write_ca_cert",
            "niks3_read_auth_mode",
            "niks3_read_client_cert",
            "niks3_read_client_key",
            "niks3_read_ca_cert",
        ] {
            updates.push(format!("{column} = ${bind_count}"));
            bind_count += 1;
        }
    }

    if updates.is_empty() && update.environment_ids.is_none() {
        // No fields to update, just return the existing record
        return Ok(Some(current));
    }

    let mut destination = if !updates.is_empty() {
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
        if update_push_to {
            q = q.bind(&niks3.push_to);
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
        if let Some(ref s3_access_key_id) = update.s3_access_key_id {
            let encrypted = cache_secrets::encrypt_secret(s3_access_key_id)?;
            q = q.bind(encrypted);
        }
        if let Some(ref s3_secret_access_key) = update.s3_secret_access_key {
            let encrypted = cache_secrets::encrypt_secret(s3_secret_access_key)?;
            q = q.bind(encrypted);
        }
        if let Some(ref s3_session_token) = update.s3_session_token {
            let encrypted = cache_secrets::encrypt_secret(s3_session_token)?;
            q = q.bind(encrypted);
        }
        if update_s3_endpoint {
            q = q.bind(&niks3.s3_endpoint_url);
        }
        if let Some(ref attic_token) = update.attic_token {
            let encrypted = cache_secrets::encrypt_secret(attic_token)?;
            q = q.bind(encrypted);
        }
        if let Some(ref attic_cache_name) = update.attic_cache_name {
            q = q.bind(attic_cache_name);
        }
        if let Some(ref attic_public_key) = update.attic_public_key {
            q = q.bind(attic_public_key);
        }
        if let Some(attic_ignore_upstream_cache_filter) = update.attic_ignore_upstream_cache_filter
        {
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

        if update_niks3 {
            q = q
                .bind(&niks3.niks3_server_url)
                .bind(&niks3.niks3_public_keys)
                .bind(&niks3.niks3_write_auth_mode)
                .bind(cache_secrets::encrypt_optional(
                    niks3.niks3_auth_token.as_deref(),
                )?)
                .bind(&niks3.niks3_write_client_cert)
                .bind(cache_secrets::encrypt_optional(
                    niks3.niks3_write_client_key.as_deref(),
                )?)
                .bind(&niks3.niks3_write_ca_cert)
                .bind(&niks3.niks3_read_auth_mode)
                .bind(&niks3.niks3_read_client_cert)
                .bind(cache_secrets::encrypt_optional(
                    niks3.niks3_read_client_key.as_deref(),
                )?)
                .bind(&niks3.niks3_read_ca_cert);
        }

        // Bind the ID for WHERE clause
        q = q.bind(id);

        q.fetch_optional(&mut *tx).await?
    } else {
        // No fields to update, get existing
        Some(current.clone())
    };

    // Update environment assignments if provided
    if let Some(ref env_ids) = update.environment_ids {
        let revision = replace_cache_environments_tx(&mut tx, id, env_ids).await?;
        if let Some(destination) = &mut destination {
            destination.updated_at = revision;
        }
    }

    tx.commit().await?;

    let destination = destination.map(decrypt_destination_secrets).transpose()?;

    if let Some(ref dest) = destination {
        debug!("Updated cache destination: {}", dest.name);
    }

    Ok(destination)
}

#[cfg(test)]
fn validate_update_shape(
    current: &CacheDestination,
    update: &UpdateCacheDestination,
) -> Result<()> {
    effective_update(current, update)
        .map(|_| ())
        .map_err(anyhow::Error::msg)
}

/// Returns the validated plaintext configuration shared by Save and Test.
///
/// Same-type omissions retain stored values. Type conversions cannot borrow
/// inactive credentials, including Niks3 modes and client identities. Mode
/// transitions and explicit clears use the same merge as persistence. This
/// function performs no encryption, database, filesystem, or network operations.
/// The caller must not serialize or log the returned plaintext credentials.
/// Same-type sanitized URL round trips retain the stored raw URL; other explicit
/// URLs cannot borrow its userinfo or queries. Inherited URLs on type conversions
/// have URI credentials removed. Save must bind these effective URL values.
///
/// # Errors
/// Returns a credential-free validation error for an invalid effective update.
///
/// # Examples
/// ```
/// use crystal_forge::models::cache_destination::{CacheDestination, UpdateCacheDestination};
/// use crystal_forge::queries::cache_destinations::effective_update;
/// let current = CacheDestination {
///     name: "public".into(), cache_type: "Nix".into(),
///     push_to: Some("https://cache.example".into()), ..Default::default()
/// };
/// assert_eq!(effective_update(&current, &UpdateCacheDestination::default())?.name, "public");
/// # Ok::<(), String>(())
/// ```
pub fn effective_update(
    current: &CacheDestination,
    update: &UpdateCacheDestination,
) -> std::result::Result<CreateCacheDestination, String> {
    let same_type = update
        .cache_type
        .as_deref()
        .is_none_or(|ty| ty == current.cache_type);
    let mut source = current.clone();
    if update
        .cache_type
        .as_deref()
        .is_some_and(|ty| ty != current.cache_type)
    {
        // SECURITY: Historical inactive fields are not a credential library.
        source.attic_token = None;
        source.s3_access_key_id = None;
        source.s3_secret_access_key = None;
        source.s3_session_token = None;
        source.niks3_auth_token = None;
        source.niks3_write_auth_mode = None;
        source.niks3_read_auth_mode = None;
        source.niks3_write_client_cert = None;
        source.niks3_write_client_key = None;
        source.niks3_write_ca_cert = None;
        source.niks3_read_client_cert = None;
        source.niks3_read_client_key = None;
        source.niks3_read_ca_cert = None;
    }
    let current = &source;
    let niks3 = current.merge_niks3_update(update)?;
    let merged = CreateCacheDestination {
        name: update.name.clone().unwrap_or_else(|| current.name.clone()),
        cache_type: update
            .cache_type
            .clone()
            .unwrap_or_else(|| current.cache_type.clone()),
        push_to: effective_cache_url(
            current.push_to.as_deref(),
            update.push_to.as_deref(),
            same_type,
        ),
        enabled: Some(update.enabled.unwrap_or(current.enabled)),
        signing_key_path: update
            .signing_key_path
            .clone()
            .or_else(|| current.signing_key_path.clone()),
        compression: update
            .compression
            .clone()
            .or_else(|| current.compression.clone()),
        s3_region: update
            .s3_region
            .clone()
            .or_else(|| current.s3_region.clone()),
        s3_profile: update
            .s3_profile
            .clone()
            .or_else(|| current.s3_profile.clone()),
        s3_access_key_id: update
            .s3_access_key_id
            .clone()
            .or_else(|| current.s3_access_key_id.clone()),
        s3_secret_access_key: update
            .s3_secret_access_key
            .clone()
            .or_else(|| current.s3_secret_access_key.clone()),
        s3_session_token: update
            .s3_session_token
            .clone()
            .or_else(|| current.s3_session_token.clone()),
        s3_endpoint_url: effective_cache_url(
            current.s3_endpoint_url.as_deref(),
            update.s3_endpoint_url.as_deref(),
            same_type,
        ),
        attic_token: update
            .attic_token
            .clone()
            .or_else(|| current.attic_token.clone()),
        attic_cache_name: update
            .attic_cache_name
            .clone()
            .or_else(|| current.attic_cache_name.clone()),
        attic_public_key: update
            .attic_public_key
            .clone()
            .or_else(|| current.attic_public_key.clone()),
        attic_ignore_upstream_cache_filter: update
            .attic_ignore_upstream_cache_filter
            .or(current.attic_ignore_upstream_cache_filter),
        attic_jobs: update.attic_jobs.or(current.attic_jobs),
        parallel_uploads: update.parallel_uploads.or(current.parallel_uploads),
        max_retries: update.max_retries.or(current.max_retries),
        retry_delay_seconds: update.retry_delay_seconds.or(current.retry_delay_seconds),
        push_timeout_seconds: update.push_timeout_seconds.or(current.push_timeout_seconds),
        force_repush: update.force_repush.or(current.force_repush),
        require_sigs: update.require_sigs.or(current.require_sigs),
        environment_ids: None,
        niks3_server_url: niks3.niks3_server_url,
        niks3_public_keys: niks3.niks3_public_keys,
        niks3_write_auth_mode: niks3.niks3_write_auth_mode,
        niks3_auth_token: niks3.niks3_auth_token,
        niks3_write_client_cert: niks3.niks3_write_client_cert,
        niks3_write_client_key: niks3.niks3_write_client_key,
        niks3_write_ca_cert: niks3.niks3_write_ca_cert,
        niks3_read_auth_mode: niks3.niks3_read_auth_mode,
        niks3_read_client_cert: niks3.niks3_read_client_cert,
        niks3_read_client_key: niks3.niks3_read_client_key,
        niks3_read_ca_cert: niks3.niks3_read_ca_cert,
    };

    merged.validate()?;
    Ok(merged)
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

/// Records destination usage without changing publication configuration.
///
/// The existing timestamp trigger also advances `updated_at`. Publication
/// evidence excludes both timestamps so usage does not invalidate a probe.
///
/// # Errors
/// Returns an error when PostgreSQL cannot record destination usage.
pub async fn update_cache_destination_last_used(pool: &PgPool, name: &str) -> Result<()> {
    sqlx::query("UPDATE cache_destinations SET last_used_at = NOW() WHERE name = $1")
        .bind(name)
        .execute(pool)
        .await?;

    debug!("Updated last_used_at for cache destination: {}", name);
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Environment Assignment Queries
// ─────────────────────────────────────────────────────────────────────────────

/// Replaces a destination's environment assignment set atomically.
///
/// Locks the destination before modifying assignments. The same lock serializes
/// create/update assignment paths and conflicts with publication snapshots.
/// Updates `updated_at` for configuration observers in the same transaction.
///
/// # Errors
/// Returns an error for a missing destination, invalid environment IDs, or a
/// database failure. An error leaves the previous assignment set unchanged.
pub async fn assign_environments_to_cache(
    pool: &PgPool,
    cache_id: i32,
    environment_ids: &[uuid::Uuid],
) -> Result<()> {
    // Start transaction
    let mut tx = pool.begin().await?;

    replace_cache_environments_tx(&mut tx, cache_id, environment_ids).await?;

    tx.commit().await?;

    debug!(
        "Assigned {} environments to cache destination {}",
        environment_ids.len(),
        cache_id
    );
    Ok(())
}

// CONCURRENCY: Every assignment writer takes the destination lock first,
// including the create path (whose inserted row is already exclusively owned).
// Publication takes FOR SHARE on the same row, preventing assignment mutation
// between evidence comparison and successful completion. IDs form a set.
async fn replace_cache_environments_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    cache_id: i32,
    environment_ids: &[uuid::Uuid],
) -> Result<chrono::DateTime<chrono::Utc>> {
    let exists =
        sqlx::query_scalar::<_, i32>("SELECT id FROM cache_destinations WHERE id = $1 FOR UPDATE")
            .bind(cache_id)
            .fetch_optional(&mut **tx)
            .await?;
    if exists.is_none() {
        anyhow::bail!("Cache destination not found");
    }
    sqlx::query("DELETE FROM cache_destination_environments WHERE cache_destination_id = $1")
        .bind(cache_id)
        .execute(&mut **tx)
        .await?;
    let mut ids = environment_ids.to_vec();
    ids.sort_unstable();
    ids.dedup();
    for id in ids {
        sqlx::query("INSERT INTO cache_destination_environments (cache_destination_id, environment_id) VALUES ($1, $2)")
            .bind(cache_id).bind(id).execute(&mut **tx).await?;
    }
    sqlx::query_scalar(
        "UPDATE cache_destinations SET updated_at = NOW() WHERE id = $1 RETURNING updated_at",
    )
    .bind(cache_id)
    .fetch_one(&mut **tx)
    .await
    .map_err(Into::into)
}

/// Reads decrypted publication settings and assignments under a shared lock.
///
/// Retains `FOR SHARE` on the destination until the caller ends the transaction.
/// Configuration updates and every assignment writer must take the conflicting
/// destination lock before making changes. Existing assignment rows are also
/// locked so foreign-key cascade deletion cannot race the snapshot. The caller
/// must not log or serialize the returned private credentials.
///
/// # Errors
/// Returns an error for query, credential decryption, or certificate validation
/// failure. Returns `None` when the destination does not exist.
///
/// # Examples
/// ```no_run
/// # async fn snapshot(pool: &sqlx::PgPool) -> anyhow::Result<()> {
/// use crystal_forge::queries::cache_destinations::{
///     get_cache_publication_snapshot_tx,
/// };
/// let mut tx = pool.begin().await?;
/// let snapshot = get_cache_publication_snapshot_tx(&mut tx, 42).await?;
/// // Inspect private settings without logging them while tx retains the lock.
/// tx.commit().await?;
/// # Ok(()) }
/// ```
pub async fn get_cache_publication_snapshot_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    cache_id: i32,
) -> Result<Option<(CacheDestination, Vec<uuid::Uuid>)>> {
    let destination = sqlx::query_as::<_, CacheDestination>(
        "SELECT * FROM cache_destinations WHERE id = $1 FOR SHARE",
    )
    .bind(cache_id)
    .fetch_optional(&mut **tx)
    .await?;
    let Some(destination) = destination else {
        return Ok(None);
    };
    let ids = sqlx::query_scalar(
        "SELECT environment_id FROM cache_destination_environments WHERE cache_destination_id = $1 ORDER BY environment_id FOR SHARE")
        .bind(cache_id).fetch_all(&mut **tx).await?;
    Ok(Some((decrypt_destination_secrets(destination)?, ids)))
}

pub async fn cache_destination_exists(pool: &PgPool, cache_id: i32) -> Result<bool> {
    let exists = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM cache_destinations WHERE id = $1)",
    )
    .bind(cache_id)
    .fetch_one(pool)
    .await?;
    Ok(exists)
}

/// Get environment IDs assigned to a cache destination
pub async fn get_cache_environments(pool: &PgPool, cache_id: i32) -> Result<Vec<uuid::Uuid>> {
    let environment_ids = sqlx::query_scalar::<_, uuid::Uuid>(
        "SELECT environment_id FROM cache_destination_environments 
         WHERE cache_destination_id = $1 
         ORDER BY environment_id",
    )
    .bind(cache_id)
    .fetch_all(pool)
    .await?;

    Ok(environment_ids)
}

/// Returns enabled assigned destinations, or globals when none are enabled.
///
/// Preserves existing name ordering with ID as a stable tie breaker. Callers
/// that consume one cache must use the first result before validating transport
/// or read configuration, and must not fall back after that validation fails.
/// Disabled assignments do not prevent global fallback. Decryption errors in
/// the selected set fail closed rather than selecting a different destination.
///
/// # Errors
/// Returns an error for database or selected-set secret decryption failures.
///
/// # Examples
/// ```no_run
/// # async fn selected(pool: &sqlx::PgPool, environment: uuid::Uuid)
/// # -> anyhow::Result<()> {
/// use crystal_forge::queries::cache_destinations::
///     eligible_cache_destinations_for_environment;
/// let destinations =
///     eligible_cache_destinations_for_environment(pool, Some(environment)).await?;
/// let selected = destinations.first();
/// assert!(selected.is_none_or(|destination| destination.enabled));
/// # Ok(()) }
/// ```
pub async fn eligible_cache_destinations_for_environment(
    pool: &PgPool,
    environment_id: Option<uuid::Uuid>,
) -> Result<Vec<CacheDestination>> {
    let destinations = sqlx::query_as::<_, CacheDestination>(
        r#"WITH assigned AS (
            SELECT cd.id FROM cache_destinations cd
            JOIN cache_destination_environments cde ON cde.cache_destination_id = cd.id
            WHERE cd.enabled = TRUE AND cde.environment_id = $1
        )
        SELECT cd.* FROM cache_destinations cd
        WHERE cd.enabled = TRUE AND (
            cd.id IN (SELECT id FROM assigned)
            OR (NOT EXISTS (SELECT 1 FROM assigned) AND NOT EXISTS (
                SELECT 1 FROM cache_destination_environments cde
                WHERE cde.cache_destination_id = cd.id
            ))
        ) ORDER BY cd.name, cd.id"#,
    )
    .bind(environment_id)
    .fetch_all(pool)
    .await?;
    destinations
        .into_iter()
        .map(decrypt_destination_secrets)
        .collect()
}

/// Get cache destinations assigned to a specific environment (includes global caches)
pub async fn get_caches_for_environment(
    pool: &PgPool,
    environment_id: uuid::Uuid,
) -> Result<Vec<CacheDestination>> {
    let caches = sqlx::query_as::<_, CacheDestination>(
        "SELECT DISTINCT cd.* FROM cache_destinations cd
         LEFT JOIN cache_destination_environments cde ON cd.id = cde.cache_destination_id
         WHERE cd.enabled = true
           AND (cde.environment_id = $1 OR cde.environment_id IS NULL)
         ORDER BY cd.name",
    )
    .bind(environment_id)
    .fetch_all(pool)
    .await?;

    let caches = caches
        .into_iter()
        .map(decrypt_destination_secrets)
        .collect::<Result<Vec<_>>>()?;

    debug!(
        "Found {} caches for environment {} (including global)",
        caches.len(),
        environment_id
    );
    Ok(caches)
}

/// Filter cache destinations by environment (excludes global if filter is applied)
pub async fn filter_caches_by_environment(
    pool: &PgPool,
    environment_id: Option<uuid::Uuid>,
) -> Result<Vec<CacheDestination>> {
    let caches = match environment_id {
        Some(env_id) => {
            // Get caches specifically assigned to this environment
            sqlx::query_as::<_, CacheDestination>(
                "SELECT DISTINCT cd.* FROM cache_destinations cd
                 INNER JOIN cache_destination_environments cde ON cd.id = cde.cache_destination_id
                 WHERE cde.environment_id = $1
                 ORDER BY cd.name",
            )
            .bind(env_id)
            .fetch_all(pool)
            .await?
            .into_iter()
            .map(decrypt_destination_secrets)
            .collect::<Result<Vec<_>>>()?
        }
        None => {
            // Get all caches (no filter)
            list_cache_destinations(pool, false).await?
        }
    };

    Ok(caches)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::security::cache_secrets::TEST_CERTIFICATE;
    use chrono::Utc;

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires isolated test database creation privileges"]
    async fn niks3_selection_assigned_first_disabled_fallback_and_stable_order(pool: PgPool) {
        let environment: uuid::Uuid = sqlx::query_scalar("INSERT INTO environments (name, description, is_active) VALUES ('selection', 'test', TRUE) RETURNING id")
            .fetch_one(&pool).await.unwrap();
        let mut ids = Vec::new();
        for (name, assigned) in [
            ("a-global", false),
            ("z-assigned", true),
            ("b-assigned", true),
        ] {
            let destination = create_cache_destination(
                &pool,
                &CreateCacheDestination {
                    name: name.into(),
                    cache_type: "Nix".into(),
                    push_to: Some("https://cache.example".into()),
                    environment_ids: assigned.then_some(vec![environment]),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
            ids.push(destination.id);
        }
        let assigned = eligible_cache_destinations_for_environment(&pool, Some(environment))
            .await
            .unwrap();
        assert_eq!(
            assigned.iter().map(|d| d.id).collect::<Vec<_>>(),
            vec![ids[2], ids[1]]
        );
        assert_eq!(
            eligible_cache_destinations_for_environment(&pool, None)
                .await
                .unwrap()[0]
                .id,
            ids[0]
        );
        sqlx::query("UPDATE cache_destinations SET enabled = FALSE WHERE id = $1")
            .bind(ids[2])
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(
            eligible_cache_destinations_for_environment(&pool, Some(environment))
                .await
                .unwrap()[0]
                .id,
            ids[1]
        );
        sqlx::query("UPDATE cache_destinations SET enabled = FALSE WHERE id = $1")
            .bind(ids[1])
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(
            eligible_cache_destinations_for_environment(&pool, Some(environment))
                .await
                .unwrap()[0]
                .id,
            ids[0]
        );
        sqlx::query("UPDATE cache_destinations SET enabled = FALSE WHERE id = $1")
            .bind(ids[0])
            .execute(&pool)
            .await
            .unwrap();
        assert!(
            eligible_cache_destinations_for_environment(&pool, Some(environment))
                .await
                .unwrap()
                .is_empty()
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires isolated test database creation privileges"]
    async fn niks3_assignment_writers_wait_for_publication_snapshot(pool: PgPool) {
        let first: uuid::Uuid = sqlx::query_scalar("INSERT INTO environments (name, description, is_active) VALUES ('assignment-first', 'test', TRUE) RETURNING id")
            .fetch_one(&pool).await.unwrap();
        let second: uuid::Uuid = sqlx::query_scalar("INSERT INTO environments (name, description, is_active) VALUES ('assignment-second', 'test', TRUE) RETURNING id")
            .fetch_one(&pool).await.unwrap();
        let destination = create_cache_destination(
            &pool,
            &CreateCacheDestination {
                name: "assignment-lock".into(),
                cache_type: "Nix".into(),
                push_to: Some("https://cache.example".into()),
                environment_ids: Some(vec![first]),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(
            get_cache_environments(&pool, destination.id).await.unwrap(),
            vec![first]
        );
        for use_update in [false, true] {
            let mut reader = pool.begin().await.unwrap();
            let reader_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
                .fetch_one(&mut *reader)
                .await
                .unwrap();
            let (_, before) = get_cache_publication_snapshot_tx(&mut reader, destination.id)
                .await
                .unwrap()
                .unwrap();
            let writer_pool = pool.clone();
            let id = destination.id;
            let mut writer = tokio::spawn(async move {
                if use_update {
                    let mut update = empty_update();
                    update.environment_ids = Some(vec![first]);
                    update_cache_destination(&writer_pool, id, &update).await?;
                } else {
                    assign_environments_to_cache(&writer_pool, id, &[second]).await?;
                }
                Ok::<_, anyhow::Error>(())
            });
            // Observe an actual PostgreSQL lock wait, not a scheduling delay.
            tokio::time::timeout(std::time::Duration::from_secs(5), async {
                loop {
                    let waiting: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM pg_stat_activity WHERE datname = current_database() AND wait_event_type = 'Lock' AND $1 = ANY(pg_blocking_pids(pid)))")
                        .bind(reader_pid).fetch_one(&pool).await.unwrap();
                    if waiting { break; }
                    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
                }
            }).await.unwrap();
            assert!(
                tokio::time::timeout(std::time::Duration::from_millis(20), &mut writer)
                    .await
                    .is_err()
            );
            assert_eq!(get_cache_environments(&pool, id).await.unwrap(), before);
            reader.commit().await.unwrap();
            tokio::time::timeout(std::time::Duration::from_secs(5), writer)
                .await
                .unwrap()
                .unwrap()
                .unwrap();
            assert_eq!(
                get_cache_environments(&pool, id).await.unwrap(),
                if use_update {
                    vec![first]
                } else {
                    vec![second]
                }
            );
        }
        // Usage still advances the general timestamp, but leaves assignment
        // and credential configuration intact for the fingerprint comparison.
        update_cache_destination_last_used(&pool, "assignment-lock")
            .await
            .unwrap();
        let used = get_cache_destination(&pool, destination.id)
            .await
            .unwrap()
            .unwrap();
        assert!(used.last_used_at.is_some());
        assert!(used.updated_at >= destination.updated_at);
        assert_eq!(
            get_cache_environments(&pool, destination.id).await.unwrap(),
            vec![first]
        );
    }

    fn empty_update() -> UpdateCacheDestination {
        UpdateCacheDestination {
            name: None,
            cache_type: None,
            push_to: None,
            enabled: None,
            signing_key_path: None,
            compression: None,
            s3_region: None,
            s3_profile: None,
            s3_access_key_id: None,
            s3_secret_access_key: None,
            s3_session_token: None,
            s3_endpoint_url: None,
            attic_token: None,
            attic_cache_name: None,
            attic_public_key: None,
            attic_ignore_upstream_cache_filter: None,
            attic_jobs: None,
            parallel_uploads: None,
            max_retries: None,
            retry_delay_seconds: None,
            push_timeout_seconds: None,
            force_repush: None,
            require_sigs: None,
            environment_ids: None,
            ..Default::default()
        }
    }

    fn base_destination() -> CacheDestination {
        CacheDestination {
            id: 1,
            name: "cache-a".to_string(),
            cache_type: "S3".to_string(),
            push_to: Some("s3://bucket/path".to_string()),
            enabled: true,
            signing_key_path: None,
            compression: None,
            s3_region: Some("us-east-1".to_string()),
            s3_profile: None,
            s3_access_key_id: Some("AKIA...".to_string()),
            s3_secret_access_key: Some("super-secret".to_string()),
            s3_session_token: None,
            s3_endpoint_url: Some("https://s3.amazonaws.com".to_string()),
            attic_token: None,
            attic_cache_name: None,
            attic_public_key: None,
            attic_ignore_upstream_cache_filter: None,
            attic_jobs: None,
            parallel_uploads: None,
            max_retries: None,
            retry_delay_seconds: None,
            push_timeout_seconds: None,
            force_repush: None,
            require_sigs: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            last_used_at: None,
            ..Default::default()
        }
    }

    fn niks3_destination() -> CacheDestination {
        CacheDestination {
            cache_type: "Niks3".into(),
            push_to: Some("https://read.example.com".into()),
            niks3_server_url: Some("https://write.example.com".into()),
            niks3_public_keys: vec![crate::models::cache_destination::nix_public_key_fixture(
                "cache-1",
            )],
            niks3_write_auth_mode: Some("token".into()),
            niks3_auth_token: Some("old-token".into()),
            niks3_read_auth_mode: Some("mtls".into()),
            niks3_read_client_cert: Some(TEST_CERTIFICATE.into()),
            niks3_read_client_key: Some("read-key".into()),
            niks3_read_ca_cert: Some(TEST_CERTIFICATE.into()),
            ..base_destination()
        }
    }

    #[test]
    fn niks3_unrelated_update_preserves_credentials() {
        let current = niks3_destination();
        let update = UpdateCacheDestination {
            name: Some("renamed".into()),
            ..empty_update()
        };
        validate_update_shape(&current, &update).unwrap();
        let merged = current.merge_niks3_update(&update).unwrap();
        assert_eq!(merged.niks3_auth_token, current.niks3_auth_token);
        assert_eq!(merged.niks3_read_client_key, current.niks3_read_client_key);
        assert_eq!(
            merged.niks3_read_client_cert,
            current.niks3_read_client_cert
        );
        assert_eq!(merged.niks3_read_ca_cert, current.niks3_read_ca_cert);
    }

    #[test]
    fn niks3_mode_transitions_clear_complete_old_credential_sets() {
        let current = niks3_destination();
        let update = UpdateCacheDestination {
            niks3_write_auth_mode: Some("mtls".into()),
            niks3_write_client_cert: Some(TEST_CERTIFICATE.into()),
            niks3_write_client_key: Some("write-key".into()),
            niks3_write_ca_cert: Some(TEST_CERTIFICATE.into()),
            niks3_read_auth_mode: Some("none".into()),
            clear_niks3_auth_token: true,
            clear_niks3_read_client_key: true,
            ..empty_update()
        };
        validate_update_shape(&current, &update).unwrap();
        let mtls = current.merge_niks3_update(&update).unwrap();
        assert!(mtls.niks3_auth_token.is_none());
        assert!(mtls.niks3_read_client_cert.is_none());
        assert!(mtls.niks3_read_client_key.is_none());
        assert!(mtls.niks3_read_ca_cert.is_none());
        let update = UpdateCacheDestination {
            niks3_write_auth_mode: Some("token".into()),
            niks3_auth_token: Some("new-token".into()),
            clear_niks3_write_client_key: true,
            ..empty_update()
        };
        validate_update_shape(&mtls, &update).unwrap();
        let token = mtls.merge_niks3_update(&update).unwrap();
        assert_eq!(token.niks3_auth_token.as_deref(), Some("new-token"));
        assert!(token.niks3_write_client_cert.is_none());
        assert!(token.niks3_write_client_key.is_none());
        assert!(token.niks3_write_ca_cert.is_none());
    }

    #[test]
    fn niks3_rejects_incomplete_transitions_and_invalid_secret_clears() {
        let current = niks3_destination();
        for update in [
            UpdateCacheDestination {
                niks3_write_auth_mode: Some("mtls".into()),
                ..empty_update()
            },
            UpdateCacheDestination {
                clear_niks3_auth_token: true,
                ..empty_update()
            },
            UpdateCacheDestination {
                clear_niks3_read_client_key: true,
                ..empty_update()
            },
            UpdateCacheDestination {
                clear_niks3_auth_token: true,
                niks3_auth_token: Some("replacement".into()),
                ..empty_update()
            },
            UpdateCacheDestination {
                clear_niks3_write_client_key: true,
                niks3_write_client_key: Some("replacement".into()),
                ..empty_update()
            },
            UpdateCacheDestination {
                clear_niks3_read_client_key: true,
                niks3_read_client_key: Some("replacement".into()),
                ..empty_update()
            },
        ] {
            assert!(validate_update_shape(&current, &update).is_err());
            assert_eq!(current.niks3_auth_token.as_deref(), Some("old-token"));
        }
        let update = UpdateCacheDestination {
            cache_type: Some("Nix".into()),
            ..empty_update()
        };
        validate_update_shape(&current, &update).unwrap();
        let merged = current.merge_niks3_update(&update).unwrap();
        assert!(merged.niks3_auth_token.is_none());
        assert!(merged.niks3_read_client_cert.is_none());
        assert!(merged.niks3_read_client_key.is_none());
        assert!(merged.niks3_read_ca_cert.is_none());
    }

    #[test]
    fn ca_clears_preserve_modes_client_credentials_and_the_other_plane() {
        let current = CacheDestination {
            niks3_write_auth_mode: Some("mtls".into()),
            niks3_auth_token: None,
            niks3_write_client_cert: Some(TEST_CERTIFICATE.into()),
            niks3_write_client_key: Some("write-key".into()),
            niks3_write_ca_cert: Some(TEST_CERTIFICATE.into()),
            ..niks3_destination()
        };
        for (write, read) in [(true, false), (false, true), (true, true)] {
            let update = UpdateCacheDestination {
                clear_niks3_write_ca_cert: write,
                clear_niks3_read_ca_cert: read,
                ..empty_update()
            };
            validate_update_shape(&current, &update).unwrap();
            let merged = current.merge_niks3_update(&update).unwrap();
            assert_eq!(merged.niks3_write_ca_cert.is_none(), write);
            assert_eq!(merged.niks3_read_ca_cert.is_none(), read);
            assert_eq!(merged.niks3_write_auth_mode, current.niks3_write_auth_mode);
            assert_eq!(merged.niks3_read_auth_mode, current.niks3_read_auth_mode);
            assert_eq!(
                merged.niks3_write_client_cert,
                current.niks3_write_client_cert
            );
            assert_eq!(
                merged.niks3_write_client_key,
                current.niks3_write_client_key
            );
            assert_eq!(
                merged.niks3_read_client_cert,
                current.niks3_read_client_cert
            );
            assert_eq!(merged.niks3_read_client_key, current.niks3_read_client_key);
        }
        for update in [
            UpdateCacheDestination {
                clear_niks3_write_ca_cert: true,
                niks3_write_ca_cert: Some(TEST_CERTIFICATE.into()),
                ..empty_update()
            },
            UpdateCacheDestination {
                clear_niks3_read_ca_cert: true,
                niks3_read_ca_cert: Some(TEST_CERTIFICATE.into()),
                ..empty_update()
            },
        ] {
            assert!(validate_update_shape(&current, &update).is_err());
        }
        let renamed = current
            .merge_niks3_update(&UpdateCacheDestination {
                name: Some("renamed".into()),
                ..empty_update()
            })
            .unwrap();
        assert_eq!(renamed.niks3_write_ca_cert, current.niks3_write_ca_cert);
        assert_eq!(renamed.niks3_read_ca_cert, current.niks3_read_ca_cert);
    }

    #[test]
    fn update_and_read_gate_reject_combined_certificate_private_key_fields() {
        let current = CacheDestination {
            niks3_write_auth_mode: Some("mtls".into()),
            niks3_auth_token: None,
            niks3_write_client_cert: Some(TEST_CERTIFICATE.into()),
            niks3_write_client_key: Some("write-key".into()),
            ..niks3_destination()
        };
        let combined = format!(
            "{TEST_CERTIFICATE}-----BEGIN PRIVATE KEY-----\nAQID\n-----END PRIVATE KEY-----"
        );
        for field in [
            "niks3_write_client_cert",
            "niks3_write_ca_cert",
            "niks3_read_client_cert",
            "niks3_read_ca_cert",
        ] {
            let update: UpdateCacheDestination =
                serde_json::from_value(serde_json::json!({ (field): combined })).unwrap();
            assert!(
                validate_update_shape(&current, &update)
                    .unwrap_err()
                    .to_string()
                    .contains(field)
            );
            let mut stored = serde_json::to_value(&current).unwrap();
            stored[field] = serde_json::Value::String(combined.clone());
            let stored: CacheDestination = serde_json::from_value(stored).unwrap();
            let error = decrypt_destination_secrets(stored).unwrap_err().to_string();
            assert!(error.contains(field));
            assert!(!error.contains("AQID"));
        }
    }

    #[test]
    fn validate_update_shape_rejects_invalid_attic_switch() {
        let current = base_destination();
        let mut update = empty_update();
        update.cache_type = Some("Attic".to_string());

        let err =
            validate_update_shape(&current, &update).expect_err("must reject invalid Attic shape");
        assert!(
            err.to_string().contains("attic_cache_name is required")
                || err.to_string().contains("attic_public_key is required")
                || err.to_string().contains("attic_token is required")
        );
    }

    #[test]
    fn validate_update_shape_accepts_valid_attic_switch() {
        let current = base_destination();
        let mut update = empty_update();
        update.cache_type = Some("Attic".to_string());
        update.push_to = Some("https://attic.example.com".to_string());
        update.attic_cache_name = Some("binary-cache".to_string());
        update.attic_public_key = Some("attic:pub:key".to_string());
        update.attic_token = Some("attic-token".to_string());

        validate_update_shape(&current, &update).expect("valid Attic update must pass");
    }
}

/// Get global cache destinations (not assigned to any environment)
pub async fn get_global_caches(pool: &PgPool) -> Result<Vec<CacheDestination>> {
    let caches = sqlx::query_as::<_, CacheDestination>(
        "SELECT cd.* FROM cache_destinations cd
         LEFT JOIN cache_destination_environments cde ON cd.id = cde.cache_destination_id
         WHERE cd.enabled = true
           AND cde.cache_destination_id IS NULL
         ORDER BY cd.name",
    )
    .fetch_all(pool)
    .await?;

    let caches = caches
        .into_iter()
        .map(decrypt_destination_secrets)
        .collect::<Result<Vec<_>>>()?;

    debug!("Found {} global cache destinations", caches.len());
    Ok(caches)
}

#[cfg(test)]
mod atomic_scope_tests {
    use super::*;
    use crate::models::cache_destination::nix_public_key_fixture;
    use crate::security::cache_secrets::TEST_CERTIFICATE;
    use uuid::Uuid;

    fn scoped_create(environment_ids: Vec<Uuid>) -> CreateCacheDestination {
        CreateCacheDestination {
            name: "atomic-scoped-niks3".into(),
            cache_type: "Niks3".into(),
            push_to: Some("https://read.example/cache".into()),
            niks3_server_url: Some("https://write.example/api".into()),
            niks3_public_keys: vec![nix_public_key_fixture("cache-1")],
            niks3_write_auth_mode: Some("token".into()),
            niks3_auth_token: Some("original-test-token".into()),
            niks3_read_auth_mode: Some("mtls".into()),
            niks3_read_client_cert: Some(TEST_CERTIFICATE.into()),
            niks3_read_client_key: Some("original-test-read-key".into()),
            environment_ids: Some(environment_ids),
            ..Default::default()
        }
    }

    async fn environment(pool: &PgPool, name: &str) -> Uuid {
        sqlx::query_scalar("INSERT INTO environments (name) VALUES ($1) RETURNING id")
            .bind(name)
            .fetch_one(pool)
            .await
            .unwrap()
    }

    fn assert_scope_foreign_key_failure(error: &anyhow::Error) {
        let database = error.downcast_ref::<sqlx::Error>().unwrap();
        assert_eq!(
            database.as_database_error().unwrap().code().as_deref(),
            Some("23503")
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires verified task PostgreSQL and ephemeral database creation"]
    async fn create_scope_failure_leaves_no_cache_credentials_or_global_fallback(pool: PgPool) {
        let valid = environment(&pool, "atomic-create-valid").await;
        // Assignment replacement sorts UUIDs. Insert the valid assignment before
        // the missing one to prove rollback of partial assignment insertion too.
        let missing = Uuid::from_u128(u128::MAX);
        let create = scoped_create(vec![valid, missing]);
        let before: i64 = sqlx::query_scalar("SELECT count(*) FROM cache_destinations")
            .fetch_one(&pool)
            .await
            .unwrap();
        let error = create_cache_destination(&pool, &create).await.unwrap_err();
        assert_scope_foreign_key_failure(&error);
        let after: i64 = sqlx::query_scalar("SELECT count(*) FROM cache_destinations")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(after, before);
        assert!(
            get_cache_destination_by_name(&pool, &create.name)
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            get_global_caches(&pool)
                .await
                .unwrap()
                .iter()
                .all(|cache| cache.name != create.name)
        );
        let assignments: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM cache_destination_environments WHERE environment_id = $1",
        )
        .bind(valid)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(assignments, 0);
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires verified task PostgreSQL and ephemeral database creation"]
    async fn update_scope_failure_preserves_entire_config_ciphertext_and_assignments(pool: PgPool) {
        let original_scope = environment(&pool, "atomic-update-original").await;
        let replacement_scope = environment(&pool, "atomic-update-replacement").await;
        let create = scoped_create(vec![original_scope]);
        let destination = create_cache_destination(&pool, &create).await.unwrap();
        let snapshot = |pool: PgPool| async move {
            sqlx::query_scalar::<_, serde_json::Value>(
                "SELECT to_jsonb(cd) FROM cache_destinations cd WHERE id = $1",
            )
            .bind(destination.id)
            .fetch_one(&pool)
            .await
            .unwrap()
        };
        let before = snapshot(pool.clone()).await;
        for field in ["niks3_auth_token", "niks3_read_client_key"] {
            assert!(cache_secrets::is_encrypted(before[field].as_str().unwrap()));
        }
        let assignments_before = get_cache_environments(&pool, destination.id).await.unwrap();
        assert_eq!(assignments_before, vec![original_scope]);
        let update = UpdateCacheDestination {
            name: Some("atomic-renamed-niks3".into()),
            push_to: Some("https://replacement-read.example/cache".into()),
            niks3_server_url: Some("https://replacement-write.example/api".into()),
            niks3_public_keys: vec![nix_public_key_fixture("replacement-key")],
            niks3_auth_token: Some("replacement-test-token".into()),
            niks3_read_client_key: Some("replacement-test-read-key".into()),
            parallel_uploads: Some(7),
            environment_ids: Some(vec![replacement_scope, Uuid::from_u128(u128::MAX)]),
            ..Default::default()
        };
        let error = update_cache_destination(&pool, destination.id, &update)
            .await
            .unwrap_err();
        assert_scope_foreign_key_failure(&error);
        // Raw row comparison includes every column and the exact randomized
        // ciphertext. A decrypt-and-compare check alone could miss re-encryption.
        assert_eq!(snapshot(pool.clone()).await, before);
        assert_eq!(
            get_cache_environments(&pool, destination.id).await.unwrap(),
            assignments_before
        );
        let restored = get_cache_destination(&pool, destination.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(restored.niks3_auth_token, create.niks3_auth_token);
        assert_eq!(restored.niks3_read_client_key, create.niks3_read_client_key);
        assert!(
            get_global_caches(&pool)
                .await
                .unwrap()
                .iter()
                .all(|cache| cache.id != destination.id)
        );
    }
}
