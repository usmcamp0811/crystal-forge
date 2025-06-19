use crate::models::flake::Flake;

pub async fn insert_flake(pool: &PgPool, name: &str, repo_url: &str) -> Result<Flake> {
    let flake = sqlx::query_as::<_, Flake>(
        "
        INSERT INTO tbl_flakes (name, repo_url)
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
    let commit = sqlx::query_as::<_, Commit>("SELECT * FROM tbl_flakes WHERE name = $1")
        .bind(name)
        .fetch_one(pool)
        .await?;

    Ok(commit)
}

pub async fn get_flake_by_id(pool: &PgPool, id: &str) -> Result<Flake> {
    let commit = sqlx::query_as::<_, Commit>("SELECT * FROM tbl_flakes WHERE id = $1")
        .bind(id)
        .fetch_one(pool)
        .await?;

    Ok(commit)
}
