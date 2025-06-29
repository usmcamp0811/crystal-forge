use crate::models::users::User;
use anyhow::{Context, Result};
use sqlx::{PgPool, Row};
use uuid::Uuid;

pub async fn create_user(pool: &PgPool, user: User) -> Result<Uuid> {
    let result = sqlx::query!(
        r#"
        INSERT INTO users (
            id, username, first_name, last_name, email, 
            user_type, is_active, created_by
        ) 
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        RETURNING id
        "#,
        user.id,
        user.username,
        user.first_name,
        user.last_name,
        user.email,
        user.user_type,
        user.is_active,
        user.created_by
    )
    .fetch_one(pool)
    .await?;

    Ok(result.id)
}

pub async fn get_by_username(pool: &PgPool, username: &str) -> Result<Option<User>> {
    let user = sqlx::query_as::<_, System>("SELECT * FROM users WHERE username = $1")
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
