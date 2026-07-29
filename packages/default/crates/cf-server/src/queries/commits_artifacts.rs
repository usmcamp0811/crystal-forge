use anyhow::Result;
use sqlx::PgPool;

/// Represents the state of cached nixosConfigurations for a commit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CachedSystemsState {
    /// No cache row exists at all — hydration has never run for this commit.
    Missing,
    /// A cache row exists but the last hydration attempt failed.
    /// Inline discovery should be retried.
    HydrationFailed,
    /// A cache row exists with systems populated by a successful discovery.
    /// The inner Vec may be empty (legitimately empty nixosConfigurations set).
    Ready(Vec<String>),
}

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
        INSERT INTO commit_artifacts_cache (commit_id, nixos_configurations, changed_files, nixos_configurations_populated, populated_at)
        VALUES ($1, $2, $3, TRUE, NOW())
        ON CONFLICT (commit_id) DO UPDATE
        SET nixos_configurations = EXCLUDED.nixos_configurations,
            changed_files = EXCLUDED.changed_files,
            nixos_configurations_populated = TRUE,
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

/// Get the known nixosConfigurations state for a commit from the artifact cache.
///
/// Returns one of:
/// - `CachedSystemsState::Missing` — no cache row exists.
/// - `CachedSystemsState::HydrationFailed` — last hydration attempt failed;
///   inline discovery should be retried.
/// - `CachedSystemsState::Ready(systems)` — successful discovery; systems may
///   be an empty vector for legitimately empty nixosConfigurations.
pub async fn get_commit_nixos_configurations_from_cache(
    pool: &PgPool,
    commit_id: i32,
) -> Result<CachedSystemsState> {
    let row = sqlx::query_as::<_, (Option<Vec<String>>, bool)>(
        r#"
        SELECT
            nixos_configurations,
            nixos_configurations_populated
        FROM commit_artifacts_cache
        WHERE commit_id = $1
        "#,
    )
    .bind(commit_id)
    .fetch_optional(pool)
    .await?;

    match row {
        Some((systems, populated)) => {
            if populated {
                let systems = systems.unwrap_or_default();
                Ok(CachedSystemsState::Ready(systems))
            } else {
                Ok(CachedSystemsState::HydrationFailed)
            }
        }
        None => Ok(CachedSystemsState::Missing),
    }
}

/// Upsert only the nixos_configurations column, preserving existing changed_files.
///
/// Unlike `upsert_commit_artifact_cache`, this does NOT overwrite changed_files
/// with an empty array. Use this for inline hydration during evaluation so that
/// previously cached changed_files are preserved.
/// Always marks `nixos_configurations_populated = true`.
pub async fn upsert_commit_artifact_systems(
    pool: &PgPool,
    commit_id: i32,
    nixos_configurations: &[String],
) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO commit_artifacts_cache (commit_id, nixos_configurations, nixos_configurations_populated, populated_at)
        VALUES ($1, $2, TRUE, NOW())
        ON CONFLICT (commit_id) DO UPDATE
        SET nixos_configurations = EXCLUDED.nixos_configurations,
            nixos_configurations_populated = TRUE,
            populated_at = NOW()
        "#,
    )
    .bind(commit_id)
    .bind(nixos_configurations)
    .execute(pool)
    .await?;

    Ok(())
}

/// Mark a commit as having failed artifact hydration.
///
/// Sets `nixos_configurations_populated = false` so that a subsequent inline
/// discovery during evaluation does not mistake this sentinel for a successful
/// empty discovery.
pub async fn mark_commit_artifact_hydration_failed(pool: &PgPool, commit_id: i32) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO commit_artifacts_cache (commit_id, nixos_configurations, changed_files, nixos_configurations_populated, populated_at)
        VALUES ($1, ARRAY[]::text[], ARRAY[]::text[], FALSE, NOW())
        ON CONFLICT (commit_id) DO UPDATE
        SET nixos_configurations_populated = FALSE,
            populated_at = NOW()
        "#,
    )
    .bind(commit_id)
    .execute(pool)
    .await?;

    Ok(())
}
