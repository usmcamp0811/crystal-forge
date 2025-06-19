pub async fn insert_flake(pool: &PgPool, name: &str, repo_url: &str) -> Result<()> {
    sqlx::query("INSERT INTO tbl_flakes (name, repo_url) VALUES ($1, $2) ON CONFLICT DO NOTHING")
        .bind(name)
        .bind(repo_url)
        .execute(pool)
        .await?;

    Ok(())
}

