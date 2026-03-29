use anyhow::Result;
use sqlx::PgPool;

use crate::models::flake_credentials::{
    CreateFlakeCredential, FlakeCredential, UpdateFlakeCredential,
};
use crate::security::flake_secrets;

fn decrypt_credential(mut credential: FlakeCredential) -> Result<FlakeCredential> {
    credential.secret_encrypted =
        flake_secrets::decrypt_optional(credential.secret_encrypted.as_deref())?;
    Ok(credential)
}

pub async fn get_flake_credential(
    pool: &PgPool,
    flake_id: i32,
) -> Result<Option<FlakeCredential>> {
    let credential = sqlx::query_as::<_, FlakeCredential>(
        "SELECT * FROM flake_credentials WHERE flake_id = $1",
    )
    .bind(flake_id)
    .fetch_optional(pool)
    .await?;

    credential.map(decrypt_credential).transpose()
}

pub async fn upsert_flake_credential(
    pool: &PgPool,
    flake_id: i32,
    create: &CreateFlakeCredential,
) -> Result<FlakeCredential> {
    let existing = get_flake_credential(pool, flake_id).await?;

    let effective_secret = resolve_secret_for_upsert(
        create.secret.as_deref(),
        existing.as_ref().and_then(|credential| credential.secret_encrypted.as_deref()),
    );

    let effective_create = CreateFlakeCredential {
        auth_type: create.auth_type.clone(),
        username: create.username.clone(),
        secret: effective_secret,
        ssh_username: create.ssh_username.clone(),
    };

    effective_create.validate().map_err(anyhow::Error::msg)?;

    let encrypted_secret = flake_secrets::encrypt_optional(effective_create.secret.as_deref())?;

    let credential = sqlx::query_as::<_, FlakeCredential>(
        r#"
        INSERT INTO flake_credentials (flake_id, auth_type, username, secret_encrypted, ssh_username)
        VALUES ($1, $2, $3, $4, $5)
        ON CONFLICT (flake_id) DO UPDATE
        SET auth_type = EXCLUDED.auth_type,
            username = EXCLUDED.username,
            secret_encrypted = EXCLUDED.secret_encrypted,
            ssh_username = EXCLUDED.ssh_username,
            updated_at = NOW()
        RETURNING *
        "#,
    )
    .bind(flake_id)
    .bind(&effective_create.auth_type)
    .bind(effective_create.username.as_deref().map(str::trim))
    .bind(encrypted_secret)
    .bind(effective_create.ssh_username.as_deref().map(str::trim))
    .fetch_one(pool)
    .await?;

    decrypt_credential(credential)
}

pub async fn update_flake_credential(
    pool: &PgPool,
    flake_id: i32,
    update: &UpdateFlakeCredential,
) -> Result<Option<FlakeCredential>> {
    let Some(current) = get_flake_credential(pool, flake_id).await? else {
        return Ok(None);
    };

    update.validate_against(&current).map_err(anyhow::Error::msg)?;

    let auth_type = update
        .auth_type
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or(current.auth_type.clone());
    let username = update.username.clone().or(current.username.clone());
    let ssh_username = update.ssh_username.clone().or(current.ssh_username.clone());
    let secret = update
        .secret
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or(current.secret_encrypted.clone());
    let encrypted_secret = flake_secrets::encrypt_optional(secret.as_deref())?;

    let credential = sqlx::query_as::<_, FlakeCredential>(
        r#"
        UPDATE flake_credentials
        SET auth_type = $2,
            username = $3,
            secret_encrypted = $4,
            ssh_username = $5,
            updated_at = NOW()
        WHERE flake_id = $1
        RETURNING *
        "#,
    )
    .bind(flake_id)
    .bind(auth_type)
    .bind(username.as_deref().map(str::trim))
    .bind(encrypted_secret)
    .bind(ssh_username.as_deref().map(str::trim))
    .fetch_optional(pool)
    .await?;

    credential.map(decrypt_credential).transpose()
}

fn resolve_secret_for_upsert(incoming: Option<&str>, existing: Option<&str>) -> Option<String> {
    incoming
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| existing.map(str::to_string))
}

pub async fn delete_flake_credential(pool: &PgPool, flake_id: i32) -> Result<bool> {
    let result = sqlx::query("DELETE FROM flake_credentials WHERE flake_id = $1")
        .bind(flake_id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn encrypt_plaintext_flake_secrets(pool: &PgPool) -> Result<u64> {
    let credentials = sqlx::query_as::<_, FlakeCredential>("SELECT * FROM flake_credentials")
        .fetch_all(pool)
        .await?;

    let mut updated = 0_u64;
    for credential in credentials {
        let Some(secret) = credential.secret_encrypted.as_deref() else {
            continue;
        };
        if flake_secrets::is_encrypted(secret) {
            continue;
        }

        let encrypted = flake_secrets::encrypt_secret(secret)?;
        sqlx::query(
            "UPDATE flake_credentials SET secret_encrypted = $2, updated_at = NOW() WHERE id = $1",
        )
        .bind(credential.id)
        .bind(encrypted)
        .execute(pool)
        .await?;
        updated += 1;
    }

    Ok(updated)
}

#[cfg(test)]
mod tests {
    use super::resolve_secret_for_upsert;

    #[test]
    fn resolve_secret_for_upsert_preserves_existing_on_blank_input() {
        let resolved = resolve_secret_for_upsert(Some("   "), Some("existing-secret"));
        assert_eq!(resolved.as_deref(), Some("existing-secret"));
    }

    #[test]
    fn resolve_secret_for_upsert_prefers_new_secret_when_present() {
        let resolved = resolve_secret_for_upsert(Some("new-secret"), Some("old-secret"));
        assert_eq!(resolved.as_deref(), Some("new-secret"));
    }
}
