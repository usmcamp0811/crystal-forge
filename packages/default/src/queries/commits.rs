use crate::models::commit::Commit;
use anyhow::{Context, Result};
use sqlx::{PgPool, Row};

pub async fn insert_commit(pool: &PgPool, commit_hash: &str, repo_url: &str) -> Result<()> {
    let flake_id: (i32,) = sqlx::query_as("SELECT id FROM tbl_flakes WHERE repo_url = $1")
        .bind(repo_url)
        .fetch_optional(pool)
        .await?
        .context("No flake entry found")?;

    sqlx::query(
        "INSERT INTO tbl_commits (flake_id, git_commit_hash, commit_timestamp)
         VALUES ($1, $2, now()) ON CONFLICT DO NOTHING",
    )
    .bind(flake_id.0)
    .bind(commit_hash)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn get_commit_by_hash(pool: &PgPool, commit_hash: &str) -> Result<Commit> {
    let commit =
        sqlx::query_as::<_, Commit>("SELECT * FROM tbl_commits WHERE git_commit_hash = $1")
            .bind(commit_hash)
            .fetch_one(pool)
            .await?;

    Ok(commit)
}

pub async fn get_commit_by_id(pool: &PgPool, id: &str) -> Result<Commit> {
    let commit = sqlx::query_as::<_, Commit>("SELECT * FROM tbl_commits WHERE id = $1")
        .bind(id)
        .fetch_one(pool)
        .await?;

    Ok(commit)
}

pub async fn get_commits_pending_evaluation(pool: &PgPool) -> Result<Vec<PendingCommit>> {
    let rows = sqlx::query!(
        r#"
        SELECT c.git_commit_hash, f.repo_url, f.name
        FROM tbl_commits c
        LEFT JOIN tbl_evaluation_targets t ON c.id = t.commit_id
        INNER JOIN tbl_flakes f ON c.flake_id = f.id
        WHERE t.commit_id IS NULL
        "#
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| PendingCommit {
            commit_hash: r.git_commit_hash,
            repo_url: r.repo_url,
            flake_name: r.name,
        })
        .collect())
}
