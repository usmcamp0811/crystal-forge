use anyhow::Result;
use sqlx::PgPool;

/// Get commits that need artifact cache population (no cache entry yet).
/// Returns up to `limit` commits, ordered by most recent first.
pub async fn get_commits_needing_artifact_cache(
    pool: &PgPool,
    limit: i64,
) -> Result<Vec<(i32, String, String)>> {
    let rows = sqlx::query_as::<_, (i32, String, String)>(
        r#"
        SELECT c.id, c.git_commit_hash, f.repo_url
        FROM commits c
        JOIN flakes f ON f.id = c.flake_id
        LEFT JOIN commit_artifacts_cache cac ON cac.commit_id = c.id
        WHERE cac.commit_id IS NULL
        ORDER BY c.commit_timestamp DESC
        LIMIT $1
        "#,
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;

    Ok(rows)
}

/// Upsert commit artifact cache entry.
pub async fn upsert_commit_artifact_cache(
    pool: &PgPool,
    commit_id: i32,
    nixos_configurations: &[String],
    changed_files: &[String],
) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO commit_artifacts_cache (commit_id, nixos_configurations, changed_files, populated_at)
        VALUES ($1, $2, $3, NOW())
        ON CONFLICT (commit_id) DO UPDATE
        SET nixos_configurations = EXCLUDED.nixos_configurations,
            changed_files = EXCLUDED.changed_files,
            populated_at = NOW()
        "#,
    )
    .bind(commit_id)
    .bind(nixos_configurations)
    .bind(changed_files)
    .execute(pool)
    .await?;

    Ok(())
}

/// Mark a commit as having failed artifact hydration (empty cache entry for retry prevention).
pub async fn mark_commit_artifact_hydration_failed(pool: &PgPool, commit_id: i32) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO commit_artifacts_cache (commit_id, nixos_configurations, changed_files, populated_at)
        VALUES ($1, ARRAY[]::text[], ARRAY[]::text[], NOW())
        ON CONFLICT (commit_id) DO NOTHING
        "#,
    )
    .bind(commit_id)
    .execute(pool)
    .await?;

    Ok(())
}
