use crate::models::config::WatchedFlake;
use crate::models::flakes::Flake;
use anyhow::Result;
use sqlx::PgPool;
use std::fmt;

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

pub async fn get_all_flakes_from_db(pool: &PgPool) -> Result<Vec<WatchedFlake>> {
    let rows = sqlx::query!("SELECT name, repo_url FROM flakes")
        .fetch_all(pool)
        .await?;

    Ok(rows
        .into_iter()
        .map(|row| WatchedFlake {
            name: row.name,
            repo_url: row.repo_url,
            auto_poll: true,
            initial_commit_depth: 5,
        })
        .collect())
}
