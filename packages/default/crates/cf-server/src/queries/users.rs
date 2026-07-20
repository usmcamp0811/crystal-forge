use crate::models::users::{User, UserType};
use anyhow::Result;
use sqlx::PgPool;
use uuid::Uuid;

pub async fn create_user(pool: &PgPool, user: User) -> Result<Uuid> {
    let result = sqlx::query!(
        r#"
        INSERT INTO users (
            id, username, first_name, last_name, email, 
            user_type, is_active
        ) 
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        RETURNING id
        "#,
        user.id,
        user.username,
        user.first_name,
        user.last_name,
        user.email,
        user.user_type as UserType,
        user.is_active,
    )
    .fetch_one(pool)
    .await?;

    Ok(result.id)
}

pub async fn get_by_username(pool: &PgPool, username: &str) -> Result<Option<User>> {
    let user = sqlx::query_as::<_, User>("SELECT * FROM users WHERE username = $1")
        .bind(username)
        .fetch_optional(pool)
        .await?;
    Ok(user)
}

pub async fn get_by_id(pool: &PgPool, id: Uuid) -> Result<Option<User>> {
    let user = sqlx::query_as::<_, User>("SELECT * FROM users WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await?;
    Ok(user)
}

pub async fn get_by_email(pool: &PgPool, email: &str) -> Result<Option<User>> {
    let user = sqlx::query_as::<_, User>("SELECT * FROM users WHERE email = $1")
        .bind(email)
        .fetch_optional(pool)
        .await?;
    Ok(user)
}

/// Get user by email, returning an error if not found.
pub async fn get_user_by_email(pool: &PgPool, email: &str) -> Result<User> {
    get_by_email(pool, email)
        .await?
        .ok_or_else(|| anyhow::anyhow!("User not found with email: {}", email))
}

/// Insert a new user with minimal fields (email and optional display name).
pub async fn insert_user(pool: &PgPool, email: &str, display_name: Option<&str>) -> Result<User> {
    let id = Uuid::new_v4();
    let username = email.split('@').next().unwrap_or(email);
    let (first_name, last_name) = match display_name {
        Some(name) => {
            let parts: Vec<&str> = name.splitn(2, ' ').collect();
            (
                Some(parts[0].to_string()),
                Some(parts.get(1).copied().unwrap_or("").to_string()),
            )
        }
        // Current schema has NOT NULL first_name/last_name.
        None => (Some(String::new()), Some(String::new())),
    };

    let user = User {
        id,
        username: username.to_string(),
        first_name,
        last_name,
        email: email.to_string(),
        user_type: UserType::Human,
        is_active: true,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };

    create_user(pool, user.clone()).await?;
    Ok(user)
}

pub async fn count_users(pool: &PgPool) -> Result<i64> {
    let count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM users")
        .fetch_one(pool)
        .await?;
    Ok(count)
}

pub async fn update_username_and_password_hash(
    pool: &PgPool,
    user_id: Uuid,
    username: &str,
    password_hash: &str,
) -> Result<()> {
    sqlx::query("UPDATE users SET username = $1, password_hash = $2 WHERE id = $3")
        .bind(username)
        .bind(password_hash)
        .bind(user_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn get_password_hash_by_user_id(pool: &PgPool, user_id: Uuid) -> Result<Option<String>> {
    let hash =
        sqlx::query_scalar::<_, Option<String>>("SELECT password_hash FROM users WHERE id = $1")
            .bind(user_id)
            .fetch_one(pool)
            .await?;
    Ok(hash)
}

pub async fn update_password_hash_by_user_id(
    pool: &PgPool,
    user_id: Uuid,
    password_hash: &str,
) -> Result<()> {
    sqlx::query("UPDATE users SET password_hash = $1 WHERE id = $2")
        .bind(password_hash)
        .bind(user_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn get_setup_wizard_dismissed(pool: &PgPool, user_id: Uuid) -> Result<bool> {
    let value = sqlx::query_scalar::<_, bool>(
        "SELECT COALESCE(setup_wizard_dismissed, false) FROM users WHERE id = $1",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await?
    .unwrap_or(false);

    Ok(value)
}

pub async fn set_setup_wizard_dismissed(
    pool: &PgPool,
    user_id: Uuid,
    dismissed: bool,
) -> Result<()> {
    sqlx::query("UPDATE users SET setup_wizard_dismissed = $1, updated_at = NOW() WHERE id = $2")
        .bind(dismissed)
        .bind(user_id)
        .execute(pool)
        .await?;

    Ok(())
}

pub async fn get_setup_wizard_agent_acknowledged(pool: &PgPool, user_id: Uuid) -> Result<bool> {
    let value = sqlx::query_scalar::<_, bool>(
        "SELECT COALESCE(setup_wizard_agent_acknowledged, false) FROM users WHERE id = $1",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await?
    .unwrap_or(false);

    Ok(value)
}

pub async fn set_setup_wizard_agent_acknowledged(
    pool: &PgPool,
    user_id: Uuid,
    acknowledged: bool,
) -> Result<()> {
    sqlx::query(
        "UPDATE users SET setup_wizard_agent_acknowledged = $1, updated_at = NOW() WHERE id = $2",
    )
    .bind(acknowledged)
    .bind(user_id)
    .execute(pool)
    .await?;

    Ok(())
}
