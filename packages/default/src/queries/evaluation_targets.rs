pub async fn update_derivation_hash(
    pool: &PgPool,
    commit_hash: &str,
    repo_url: &str,
    target_type: &str,
    hash: &str,
) -> Result<()> {
    let flake_id: (i32,) = sqlx::query_as("SELECT id FROM tbl_flakes WHERE repo_url = $1")
        .bind(repo_url)
        .fetch_optional(pool)
        .await?
        .context("No flake entry found")?;

    let commit_id: (i32,) =
        sqlx::query_as("SELECT id FROM tbl_commits WHERE flake_id = $1 AND git_commit_hash = $2")
            .bind(flake_id.0)
            .bind(commit_hash)
            .fetch_optional(pool)
            .await?
            .context("No commit entry found")?;

    let updated = sqlx::query(
        r#"UPDATE tbl_evaluation_targets
           SET hash = $3
           WHERE commit_id = $1 AND type = $2 AND hash IS NULL"#,
    )
    .bind(commit_id.0)
    .bind(target_type)
    .bind(hash)
    .execute(pool)
    .await?
    .rows_affected();

    if updated == 0 {
        warn!("no rows updated");
    }

    Ok(())
}

pub async fn insert_derivation(
    pool: &PgPool,
    commit_hash: &str,
    repo_url: &str,
    target_type: &str,
) -> Result<()> {
    let flake_id: (i32,) = sqlx::query_as("SELECT id FROM tbl_flakes WHERE repo_url = $1")
        .bind(repo_url)
        .fetch_optional(pool)
        .await?
        .context("No flake entry found")?;

    let commit_id: (i32,) =
        sqlx::query_as("SELECT id FROM tbl_commits WHERE flake_id = $1 AND git_commit_hash = $2")
            .bind(flake_id.0)
            .bind(commit_hash)
            .fetch_optional(pool)
            .await?
            .context("No commit found")?;

    sqlx::query(
        r#"INSERT INTO tbl_evaluation_targets (commit_id, type)
           VALUES ($1, $2) ON CONFLICT (commit_id, type) DO NOTHING"#,
    )
    .bind(commit_id.0)
    .bind(target_type)
    .execute(pool)
    .await?;

    Ok(())
}
