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
    create.validate().map_err(anyhow::Error::msg)?;

    let encrypted_secret = flake_secrets::encrypt_optional(create.secret.as_deref())?;

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
    .bind(&create.auth_type)
    .bind(create.username.as_deref().map(str::trim))
    .bind(encrypted_secret)
    .bind(create.ssh_username.as_deref().map(str::trim))
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

    let auth_type = update.auth_type.clone().unwrap_or(current.auth_type.clone());
    let username = update.username.clone().or(current.username.clone());
    let ssh_username = update.ssh_username.clone().or(current.ssh_username.clone());
    let secret = update.secret.clone().or(current.secret_encrypted.clone());
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
