use crate::config::{FlakeConfig, WatchedFlake};
use crate::models::flakes::Flake;
use anyhow::Context;
use anyhow::Result;
use sqlx::PgPool;

pub async fn insert_flake(pool: &PgPool, name: &str, repo_url: &str) -> Result<Flake> {
    let flake = sqlx::query_as::<_, Flake>(
        "
        INSERT INTO flakes (name, repo_url)
        VALUES ($1, $2)
        ON CONFLICT (repo_url) DO UPDATE SET name = EXCLUDED.name
        RETURNING *
        ",
    )
    .bind(name)
    .bind(repo_url)
    .fetch_one(pool)
    .await?;

    Ok(flake)
}

pub async fn get_flake_by_name(pool: &PgPool, name: &str) -> Result<Flake> {
    let commit = sqlx::query_as::<_, Flake>("SELECT * FROM flakes WHERE name = $1")
        .bind(name)
        .fetch_one(pool)
        .await?;

    Ok(commit)
}

pub async fn get_flake_by_id(pool: &PgPool, id: i32) -> Result<Flake> {
    let commit = sqlx::query_as::<_, Flake>("SELECT * FROM flakes WHERE id = $1")
        .bind(id)
        .fetch_one(pool)
        .await?;

    Ok(commit)
}

pub async fn get_flake_id_by_repo_url(pool: &PgPool, repo_url: &str) -> Result<Option<i32>> {
    let flake_id = sqlx::query_scalar!("SELECT id FROM flakes WHERE repo_url = $1", repo_url)
        .fetch_optional(pool)
        .await?;

    Ok(flake_id)
}

pub async fn get_all_flakes_from_db(
    pool: &PgPool,
    config: &FlakeConfig,
) -> Result<Vec<WatchedFlake>> {
    let rows = sqlx::query!("SELECT name, repo_url FROM flakes")
        .fetch_all(pool)
        .await?;

    Ok(rows
        .into_iter()
        .map(|row| {
            // Look for matching config flake to get the proper initial_commit_depth
            let config_flake = config.watched.iter().find(|f| f.repo_url == row.repo_url);

            WatchedFlake {
                name: row.name,
                repo_url: row.repo_url,
                auto_poll: true,
                initial_commit_depth: config_flake.map(|f| f.initial_commit_depth).unwrap_or(5), // fallback to 5 for database-only flakes
            }
        })
        .collect())
}

pub async fn find_flake_by_repo_urls(
    pool: &PgPool,
    possible_urls: &[String],
    preferred_url: &str,
) -> Result<Option<crate::models::flakes::Flake>> {
    sqlx::query_as!(
        crate::models::flakes::Flake,
        r#"
        SELECT id, name, repo_url
        FROM flakes 
        WHERE repo_url = ANY($1)
        ORDER BY 
            CASE 
                WHEN repo_url = $2 THEN 1  -- Exact match first
                ELSE 2
            END
        LIMIT 1
        "#,
        possible_urls,
        preferred_url
    )
    .fetch_optional(pool)
    .await
    .context("Failed to find flake by repo URLs")
}
