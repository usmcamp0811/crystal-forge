use crate::api::models::{
    BuildStatus, CommitMetadata, FlakeCommit, FlakeRegistryItem, FlakeTimeline,
};
use crate::config::{FlakeConfig, WatchedFlake};
use crate::models::flakes::{BranchCommitSnapshot, Flake};
use anyhow::Context;
use anyhow::Result;
use sqlx::PgPool;

pub async fn insert_flake(
    pool: &PgPool,
    name: &str,
    repo_url: &str,
    branch: &str,
    build_scope: &str,
) -> Result<Flake> {
    let flake = sqlx::query_as::<_, Flake>(
        "
        INSERT INTO flakes (name, repo_url, branch, build_scope)
        VALUES ($1, $2, $3, $4)
        ON CONFLICT (repo_url) DO UPDATE SET 
            name = EXCLUDED.name, 
            branch = EXCLUDED.branch,
            build_scope = EXCLUDED.build_scope,
            deleted_at = NULL
        RETURNING *
        ",
    )
    .bind(name)
    .bind(repo_url)
    .bind(branch)
    .bind(build_scope)
    .fetch_one(pool)
    .await?;

    Ok(flake)
}

pub async fn get_flake_by_name(pool: &PgPool, name: &str) -> Result<Flake> {
    let commit =
        sqlx::query_as::<_, Flake>("SELECT * FROM flakes WHERE name = $1 AND deleted_at IS NULL")
            .bind(name)
            .fetch_one(pool)
            .await?;

    Ok(commit)
}

pub async fn get_flake_by_id(pool: &PgPool, id: i32) -> Result<Flake> {
    let commit =
        sqlx::query_as::<_, Flake>("SELECT * FROM flakes WHERE id = $1 AND deleted_at IS NULL")
            .bind(id)
            .fetch_one(pool)
            .await?;

    Ok(commit)
}

pub async fn update_flake(
    pool: &PgPool,
    flake_id: i32,
    name: &str,
    repo_url: &str,
    branch: &str,
    build_scope: &str,
) -> Result<Flake> {
    let flake = sqlx::query_as::<_, Flake>(
        r#"
        UPDATE flakes
        SET name = $1,
            repo_url = $2,
            branch = $3,
            build_scope = $4
        WHERE id = $5 AND deleted_at IS NULL
        RETURNING *
        "#,
    )
    .bind(name)
    .bind(repo_url)
    .bind(branch)
    .bind(build_scope)
    .bind(flake_id)
    .fetch_one(pool)
    .await?;

    Ok(flake)
}

pub async fn get_flake_id_by_repo_url(pool: &PgPool, repo_url: &str) -> Result<Option<i32>> {
    let flake_id = sqlx::query_scalar!(
        "SELECT id FROM flakes WHERE repo_url = $1 AND deleted_at IS NULL",
        repo_url
    )
    .fetch_optional(pool)
    .await?;

    Ok(flake_id)
}

pub async fn get_all_flakes_from_db(
    pool: &PgPool,
    config: &FlakeConfig,
) -> Result<Vec<WatchedFlake>> {
    let (flakes, _ids) = get_all_flakes_from_db_with_ids(pool, config).await?;
    Ok(flakes)
}

/// Returns both the `WatchedFlake` list and a parallel vec of database flake IDs.
pub async fn get_all_flakes_from_db_with_ids(
    pool: &PgPool,
    config: &FlakeConfig,
) -> Result<(Vec<WatchedFlake>, Vec<Option<i32>>)> {
    // Use query_as so we don't require an updated sqlx offline cache.
    let rows = sqlx::query_as::<_, (i32, String, String, String)>(
        "SELECT id, name, repo_url, branch FROM flakes WHERE deleted_at IS NULL",
    )
    .fetch_all(pool)
    .await?;

    let mut flakes = Vec::with_capacity(rows.len());
    let mut ids: Vec<Option<i32>> = Vec::with_capacity(rows.len());

    for (id, name, repo_url, branch) in rows {
        let config_flake = config.watched.iter().find(|f| f.repo_url == repo_url);
        flakes.push(WatchedFlake {
            name,
            repo_url,
            branch: Some(branch),
            auto_poll: true,
            initial_commit_depth: config_flake.map(|f| f.initial_commit_depth).unwrap_or(5),
        });
        ids.push(Some(id));
    }

    Ok((flakes, ids))
}

pub async fn find_flake_by_repo_urls(
    pool: &PgPool,
    possible_urls: &[String],
    preferred_url: &str,
) -> Result<Option<crate::models::flakes::Flake>> {
    sqlx::query_as::<_, crate::models::flakes::Flake>(
        r#"
        SELECT id, name, repo_url, branch, build_scope, deleted_at
        FROM flakes 
        WHERE repo_url = ANY($1) AND deleted_at IS NULL
        ORDER BY 
            CASE 
                WHEN repo_url = $2 THEN 1  -- Exact match first
                ELSE 2
            END
        LIMIT 1
        "#,
    )
    .bind(possible_urls)
    .bind(preferred_url)
    .fetch_optional(pool)
    .await
    .context("Failed to find flake by repo URLs")
}

pub async fn list_flake_registry(pool: &PgPool) -> Result<Vec<FlakeRegistryItem>> {
    // Intermediate row struct matching the query column names.
    // Required because the tuple approach becomes unwieldy with 20+ columns.
    #[derive(sqlx::FromRow)]
    struct FlakeRegistryRow {
        id: i32,
        name: String,
        repo_url: String,
        branch: String,
        build_scope: String,
        system_count: i64,
        sync_status: String,
        last_sync_at: Option<chrono::DateTime<chrono::Utc>>,
        last_sync_error: Option<String>,
        // Enriched fields (TASK-397)
        latest_commit_hash: Option<String>,
        latest_commit_message: Option<String>,
        latest_commit_author: Option<String>,
        latest_commit_timestamp: Option<chrono::DateTime<chrono::Utc>>,
        build_status: Option<String>,
        evaluation_status: Option<String>,
        environments: Vec<String>,
        total_commit_count: i64,
    }

    let rows = sqlx::query_as::<_, FlakeRegistryRow>(
        r#"
        SELECT
            f.id,
            f.name,
            f.repo_url,
            f.branch,
            f.build_scope,
            COUNT(DISTINCT s.id)::bigint AS system_count,
            CASE
                WHEN f.sync_status = 'syncing'
                 AND f.last_sync_at IS NOT NULL
                 AND f.last_sync_at < now() - interval '30 minutes'
                THEN 'error'
                ELSE f.sync_status
            END AS sync_status,
            f.last_sync_at,
            CASE
                WHEN f.sync_status = 'syncing'
                 AND f.last_sync_at IS NOT NULL
                 AND f.last_sync_at < now() - interval '30 minutes'
                THEN COALESCE(f.last_sync_error, 'Sync appears stale — previous sync attempt did not finish')
                ELSE f.last_sync_error
            END AS last_sync_error,
            -- Latest visible commit hash from snapshot (when ready) or fallback
            COALESCE(
                snap_latest.git_commit_hash,
                fallback_latest.git_commit_hash
            ) AS latest_commit_hash,
            COALESCE(
                snap_latest.message,
                fallback_latest.message
            ) AS latest_commit_message,
            COALESCE(
                snap_latest.author,
                fallback_latest.author
            ) AS latest_commit_author,
            COALESCE(
                snap_latest.commit_timestamp,
                fallback_latest.commit_timestamp
            ) AS latest_commit_timestamp,
            -- Build status for whichever latest commit is active
            latest_build.build_status,
            COALESCE(
                snap_latest.evaluation_status,
                fallback_latest.evaluation_status
            ) AS evaluation_status,
            -- Sorted, deduplicated environment names from active systems
            COALESCE(env_agg.environments, ARRAY[]::text[]) AS environments,
            -- Total visible commits (snapshot count when ready, else commit count)
            COALESCE(snap_count.total_count, all_count.total_count, 0::bigint) AS total_commit_count
        FROM flakes f
        LEFT JOIN systems s ON s.flake_id = f.id

        -- Latest commit via branch snapshot (when ready)
        LEFT JOIN LATERAL (
            SELECT c.id, c.git_commit_hash, c.message, c.author,
                   c.commit_timestamp, c.evaluation_status
            FROM flake_branch_commit_snapshot fbcs
            JOIN commits c ON c.id = fbcs.commit_id
            WHERE fbcs.flake_id = f.id
              AND f.snapshot_ready_at IS NOT NULL
            ORDER BY fbcs.position ASC
            LIMIT 1
        ) snap_latest ON TRUE

        -- Latest commit via timestamp fallback (when snapshot not ready)
        LEFT JOIN LATERAL (
            SELECT c.id, c.git_commit_hash, c.message, c.author,
                   c.commit_timestamp, c.evaluation_status
            FROM commits c
            WHERE c.flake_id = f.id
            ORDER BY c.commit_timestamp DESC, c.id DESC
            LIMIT 1
        ) fallback_latest ON TRUE

        -- Build status for the effective latest commit
        LEFT JOIN LATERAL (
            SELECT
                CASE
                    WHEN COUNT(*) FILTER (WHERE bj.status = 'building') > 0 THEN 'building'
                    WHEN COUNT(*) FILTER (WHERE bj.status = 'queued') > 0 THEN 'queued'
                    WHEN COUNT(*) FILTER (WHERE bj.status = 'failed') > 0 THEN 'failed'
                    WHEN COUNT(*) FILTER (WHERE bj.status = 'success') > 0 THEN 'complete'
                    ELSE NULL
                END AS build_status
            FROM build_jobs bj
            JOIN derivations d ON d.id = bj.derivation_id
            WHERE d.commit_id = COALESCE(snap_latest.id, fallback_latest.id)
        ) latest_build ON TRUE

        -- Environment names from active systems (no 500-row cap)
        LEFT JOIN LATERAL (
            SELECT array_agg(DISTINCT e.name ORDER BY e.name) AS environments
            FROM systems s2
            JOIN environments e ON e.id = s2.environment_id
            WHERE s2.flake_id = f.id
              AND s2.is_active = TRUE
              AND e.name IS NOT NULL
        ) env_agg ON TRUE

        -- Snapshot commit count (when ready)
        LEFT JOIN LATERAL (
            SELECT COUNT(*)::bigint AS total_count
            FROM flake_branch_commit_snapshot fbcs2
            WHERE fbcs2.flake_id = f.id
        ) snap_count ON TRUE

        -- All commits count (fallback)
        LEFT JOIN LATERAL (
            SELECT COUNT(*)::bigint AS total_count
            FROM commits c2
            WHERE c2.flake_id = f.id
        ) all_count ON TRUE

        WHERE f.deleted_at IS NULL
        GROUP BY f.id, f.name, f.repo_url, f.branch, f.build_scope,
                 f.sync_status, f.last_sync_at, f.last_sync_error,
                 snap_latest.git_commit_hash, snap_latest.message,
                 snap_latest.author, snap_latest.commit_timestamp,
                 snap_latest.evaluation_status,
                 fallback_latest.git_commit_hash, fallback_latest.message,
                 fallback_latest.author, fallback_latest.commit_timestamp,
                 fallback_latest.evaluation_status,
                 latest_build.build_status,
                 env_agg.environments,
                 snap_count.total_count, all_count.total_count
        ORDER BY lower(f.name) ASC
        "#,
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| FlakeRegistryItem {
            id: row.id,
            name: row.name,
            repo_url: row.repo_url,
            branch: row.branch,
            build_scope: row.build_scope,
            system_count: row.system_count,
            sync_status: row.sync_status,
            last_sync_at: row.last_sync_at,
            last_sync_error: row.last_sync_error,
            latest_commit_hash: row.latest_commit_hash,
            latest_commit_message: row.latest_commit_message,
            latest_commit_author: row.latest_commit_author,
            latest_commit_timestamp: row.latest_commit_timestamp,
            build_status: row.build_status,
            evaluation_status: row.evaluation_status,
            environments: row.environments,
            total_commit_count: row.total_commit_count,
        })
        .collect())
}

pub async fn count_systems_for_flake(pool: &PgPool, flake_id: i32) -> Result<i64> {
    let system_count = sqlx::query_scalar::<_, i64>(
        r#"
        SELECT COUNT(*)::bigint
        FROM systems
        WHERE flake_id = $1
        "#,
    )
    .bind(flake_id)
    .fetch_one(pool)
    .await?;

    Ok(system_count)
}

pub async fn delete_flake_by_id(pool: &PgPool, flake_id: i32) -> Result<u64> {
    let result = sqlx::query("DELETE FROM flakes WHERE id = $1")
        .bind(flake_id)
        .execute(pool)
        .await?;

    Ok(result.rows_affected())
}

pub async fn purge_flake_commit_history(pool: &PgPool, flake_id: i32) -> Result<u64> {
    let mut tx = pool.begin().await?;

    // Clear commit-scoped caches first for deterministic cleanup.
    sqlx::query(
        r#"
        DELETE FROM commit_artifacts_cache cac
        USING commits c
        WHERE cac.commit_id = c.id
          AND c.flake_id = $1
        "#,
    )
    .bind(flake_id)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        r#"
        DELETE FROM commit_metadata_cache cmc
        USING commits c
        WHERE cmc.commit_id = c.id
          AND c.flake_id = $1
        "#,
    )
    .bind(flake_id)
    .execute(&mut *tx)
    .await?;

    // Remove derivations linked to this flake's commits.
    sqlx::query(
        r#"
        DELETE FROM derivations d
        USING commits c
        WHERE d.commit_id = c.id
          AND c.flake_id = $1
        "#,
    )
    .bind(flake_id)
    .execute(&mut *tx)
    .await?;

    let deleted_commits = sqlx::query(
        r#"
        DELETE FROM commits
        WHERE flake_id = $1
        "#,
    )
    .bind(flake_id)
    .execute(&mut *tx)
    .await?
    .rows_affected();

    tx.commit().await?;
    Ok(deleted_commits)
}

/// Soft delete a flake by setting deleted_at timestamp.
/// The flake will be excluded from normal queries but retained for audit.
pub async fn soft_delete_flake(pool: &PgPool, flake_id: i32) -> Result<u64> {
    let result =
        sqlx::query("UPDATE flakes SET deleted_at = NOW() WHERE id = $1 AND deleted_at IS NULL")
            .bind(flake_id)
            .execute(pool)
            .await?;

    Ok(result.rows_affected())
}

/// Check if flake has active dependencies (pending/in-progress evaluations, builds, or deployments).
/// Returns count of blocking dependencies.
pub async fn check_flake_dependencies(pool: &PgPool, flake_id: i32) -> Result<i64> {
    // Check if any active systems are using this flake
    //
    // NOTE: The 'evaluations' and 'build_queue' tables are planned features
    // but not yet implemented. When they are added, expand this check to include:
    // - Active evaluations (evaluations.status IN ('pending', 'in_progress'))
    // - Active builds (build_queue.status IN ('pending', 'in_progress'))
    //
    // For now, we only check for systems using the flake, which is the most
    // critical dependency that would break if we deleted the flake.
    let count = sqlx::query_scalar::<_, i64>(
        r#"
        SELECT COUNT(*)::bigint
        FROM systems
        WHERE flake_id = $1
          AND is_active = true
        "#,
    )
    .bind(flake_id)
    .fetch_one(pool)
    .await?;

    Ok(count)
}

/// Cascade delete a flake and all related data (evaluations, builds, deployments).
/// This is a hard delete that permanently removes all traces.
/// MUST be run in a transaction for safety - pass a transaction reference.
pub async fn cascade_delete_flake(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    flake_id: i32,
) -> Result<u64> {
    // Note: ON DELETE CASCADE on commits FK will handle most cleanup
    // But we explicitly delete systems first to be safe
    sqlx::query("DELETE FROM systems WHERE flake_id = $1")
        .bind(flake_id)
        .execute(&mut **tx)
        .await?;

    // Delete the flake (commits, evaluations, builds cascade automatically)
    let result = sqlx::query("DELETE FROM flakes WHERE id = $1")
        .bind(flake_id)
        .execute(&mut **tx)
        .await?;

    Ok(result.rows_affected())
}

/// Fetch flake timelines for dashboard view (CF system deployment counts).
///
/// Returns up to `max_commits_per_flake` most recent commits for each flake,
/// showing count of Crystal Forge systems deployed at each commit.
pub async fn fetch_dashboard_flake_timelines(
    pool: &PgPool,
    max_commits_per_flake: i64,
    flake_ids: Option<&[i32]>,
) -> Result<Vec<FlakeTimeline>> {
    let flake_filter: Option<Vec<i32>> = flake_ids.map(|ids| ids.to_vec());
    let flakes = sqlx::query_as::<_, (i32, String, String)>(
        "SELECT id, name, repo_url FROM flakes WHERE deleted_at IS NULL AND ($1::int[] IS NULL OR id = ANY($1)) ORDER BY name ASC",
    )
    .bind(&flake_filter)
    .fetch_all(pool)
    .await?;

    let mut timelines = Vec::new();

    for (flake_id, flake_name, repo_url) in flakes {
        let commits_rows = sqlx::query_as::<
            _,
            (
                i32,
                String,
                chrono::DateTime<chrono::Utc>,
                i64,
                Vec<String>,
                i64,
                Option<String>,
                Option<String>,
            ),
        >(
            r#"
            SELECT
                c.id,
                c.git_commit_hash,
                c.commit_timestamp,
                COALESCE(
                    (
                        SELECT COUNT(DISTINCT s.hostname)::bigint
                        FROM view_system_deployment_status s
                        WHERE s.current_commit_hash = c.git_commit_hash
                    ),
                    0
                ) AS system_count,
                COALESCE(
                    (
                        SELECT ARRAY_AGG(DISTINCT s.hostname ORDER BY s.hostname)
                        FROM view_system_deployment_status s
                        WHERE s.current_commit_hash = c.git_commit_hash
                    ),
                    ARRAY[]::text[]
                ) AS systems,
                (
                    SELECT COUNT(*)::bigint
                    FROM commits c2
                    WHERE c2.flake_id = c.flake_id
                    AND c2.commit_timestamp > c.commit_timestamp
                ) AS commits_behind,
                (
                    SELECT
                        CASE
                            WHEN COUNT(*) FILTER (WHERE bj.status = 'building') > 0 THEN 'building'
                            WHEN COUNT(*) FILTER (WHERE bj.status = 'queued') > 0 THEN 'queued'
                            WHEN COUNT(*) FILTER (WHERE bj.status = 'failed') > 0 THEN 'failed'
                            WHEN COUNT(*) FILTER (WHERE bj.status = 'success') > 0 THEN 'complete'
                            ELSE NULL
                        END
                    FROM build_jobs bj
                    JOIN derivations d ON d.id = bj.derivation_id
                    WHERE d.commit_id = c.id
                ) AS build_status,
                (
                    SELECT
                        CASE
                            WHEN COUNT(*) FILTER (WHERE d.status_id = 4) > 0 THEN 'running'
                            WHEN COUNT(*) FILTER (WHERE d.status_id = 3) > 0 THEN 'queued'
                            WHEN COUNT(*) FILTER (WHERE d.status_id = 6) > 0 THEN 'failed'
                            WHEN COUNT(*) FILTER (WHERE d.status_id = 5) > 0 THEN 'complete'
                            ELSE 'idle'
                        END
                    FROM derivations d
                    WHERE d.commit_id = c.id
                ) AS evaluation_status
            FROM commits c
            WHERE c.flake_id = $1
            ORDER BY c.commit_timestamp DESC
            LIMIT $2
            "#,
        )
        .bind(flake_id)
        .bind(max_commits_per_flake)
        .fetch_all(pool)
        .await?;

        let commits: Vec<FlakeCommit> = commits_rows
            .into_iter()
            .map(
                |(
                    id,
                    hash,
                    committed_at,
                    system_count,
                    systems,
                    commits_behind,
                    build_status,
                    evaluation_status,
                )| {
                    let build_status = build_status.as_deref().map(|status| match status {
                        "queued" => BuildStatus::Queued,
                        "building" => BuildStatus::Building,
                        "failed" => BuildStatus::Failed,
                        "complete" => BuildStatus::Complete,
                        _ => BuildStatus::Idle,
                    });

                    FlakeCommit {
                        id,
                        hash,
                        message: "".to_string(),
                        author: "".to_string(),
                        committed_at,
                        system_count,
                        commits_behind,
                        systems,
                        system_paths: Vec::new(),
                        build_status,
                        evaluation_status,
                        evaluation_error_message: None,
                        metadata: None, // Dashboard view doesn't need metadata
                    }
                },
            )
            .collect();

        timelines.push(FlakeTimeline {
            flake_id,
            flake_name,
            repo_url,
            commits,
        });
    }

    Ok(timelines)
}

/// Fetch flake timelines for flakes view (nixosConfigurations in flake).
///
/// Returns up to `max_commits_per_flake` most recent commits for each flake.
///
/// **Database-only read path** (TASK-397): Uses the branch-commit snapshot for
/// commit ordering when available (snapshot_ready_at IS NOT NULL), falling back to
/// deterministic timestamp ordering for flakes that have never been synchronized
/// since migration 0178. No Git/network operations are performed.
///
/// Commit visibility and order are derived from:
///   - `flake_branch_commit_snapshot` when the flake has been synchronized since
///     migration 0178 (snapshot mode), removing force-pushed commits from view
///     while keeping them in the commits audit table.
///   - `commits.commit_timestamp DESC, id DESC` when no snapshot exists (fallback
///     mode), preserving the pre-migration ordering behavior.
pub async fn fetch_flake_timelines(
    pool: &PgPool,
    max_commits_per_flake: i64,
    flake_ids: Option<&[i32]>,
) -> Result<Vec<FlakeTimeline>> {
    // Fetch flakes with their snapshot readiness
    let flake_filter: Option<Vec<i32>> = flake_ids.map(|ids| ids.to_vec());
    let flakes = sqlx::query_as::<_, (i32, String, String, Option<chrono::DateTime<chrono::Utc>>)>(
        "SELECT id, name, repo_url, snapshot_ready_at \
         FROM flakes WHERE deleted_at IS NULL \
         AND ($1::int[] IS NULL OR id = ANY($1)) \
         ORDER BY name ASC",
    )
    .bind(&flake_filter)
    .fetch_all(pool)
    .await?;

    let mut timelines = Vec::new();

    for (flake_id, flake_name, repo_url, snapshot_ready_at) in flakes {
        let use_snapshot = snapshot_ready_at.is_some();

        // Single query that transparently handles both snapshot and fallback modes.
        // When snapshot mode is active (snapshot_ready_at IS NOT NULL):
        //   - Only commits present in flake_branch_commit_snapshot are returned
        //   - Ordering and commits_behind use the snapshot position
        // When fallback mode (snapshot_ready_at IS NULL):
        //   - All commits are returned ordered by timestamp DESC
        //   - commits_behind uses a correlated count
        //
        // The LEFT JOIN + WHERE filter ensures mutually exclusive mode selection:
        //   - Snapshot mode: snapshot_ready_at IS NOT NULL AND fbcs.commit_id IS NOT NULL
        //   - Fallback mode: snapshot_ready_at IS NULL

        #[derive(sqlx::FromRow)]
        struct FlakeCommitRow {
            id: i32,
            git_commit_hash: String,
            commit_timestamp: chrono::DateTime<chrono::Utc>,
            message: Option<String>,
            author: Option<String>,
            system_count: i64,
            systems: Vec<String>,
            commits_behind: i64,
            build_status: Option<String>,
            evaluation_status: Option<String>,
            evaluation_error_message: Option<String>,
            total_systems: Option<i32>,
            systems_passed_policy: Option<i32>,
            systems_failed_policy_strict: Option<i32>,
            systems_failed_policy_non_strict: Option<i32>,
            has_nix_eval_error: Option<bool>,
            has_policy_failures: Option<bool>,
            all_systems_passed: Option<bool>,
        }

        // Use a single query that adapts to the flake's snapshot readiness.
        // The key differences between modes are handled by the LEFT JOIN on
        // flake_branch_commit_snapshot, the WHERE filter, and the ORDER BY.
        let commits_rows = sqlx::query_as::<_, FlakeCommitRow>(
            r#"
            SELECT
                c.id,
                c.git_commit_hash,
                c.commit_timestamp,
                c.message,
                c.author,
                COALESCE(CARDINALITY(cac.nixos_configurations), 0)::bigint AS system_count,
                COALESCE(
                    cac.nixos_configurations,
                    (
                        SELECT COALESCE(array_agg(dn.derivation_name), ARRAY[]::text[])
                        FROM (
                            SELECT DISTINCT d.derivation_name
                            FROM derivations d
                            WHERE d.commit_id = c.id
                                AND d.derivation_type = 'nixos'
                            ORDER BY d.derivation_name
                        ) dn
                    ),
                    ARRAY[]::text[]
                ) AS systems,
                CASE
                    WHEN f3.snapshot_ready_at IS NOT NULL
                    THEN fbcs.position::bigint
                    ELSE (
                        SELECT COUNT(*)::bigint
                        FROM commits c2
                        WHERE c2.flake_id = c.flake_id
                        AND c2.commit_timestamp > c.commit_timestamp
                    )
                END AS commits_behind,
                (
                    SELECT
                        CASE
                            WHEN COUNT(*) FILTER (WHERE bj.status = 'building') > 0 THEN 'building'
                            WHEN COUNT(*) FILTER (WHERE bj.status = 'queued') > 0 THEN 'queued'
                            WHEN COUNT(*) FILTER (WHERE bj.status = 'failed') > 0 THEN 'failed'
                            WHEN COUNT(*) FILTER (WHERE bj.status = 'success') > 0 THEN 'complete'
                            ELSE NULL
                        END
                    FROM build_jobs bj
                    JOIN derivations d ON d.id = bj.derivation_id
                    WHERE d.commit_id = c.id
                ) AS build_status,
                c.evaluation_status,
                c.evaluation_error_message,
                cmc.total_systems,
                cmc.systems_passed_policy,
                cmc.systems_failed_policy_strict,
                cmc.systems_failed_policy_non_strict,
                cmc.has_nix_eval_error,
                cmc.has_policy_failures,
                cmc.all_systems_passed
            FROM commits c
            JOIN flakes f3 ON f3.id = c.flake_id AND f3.id = $1 AND f3.deleted_at IS NULL
            LEFT JOIN flake_branch_commit_snapshot fbcs
                ON fbcs.commit_id = c.id AND fbcs.flake_id = c.flake_id
            LEFT JOIN commit_artifacts_cache cac ON cac.commit_id = c.id
            LEFT JOIN commit_metadata_cache cmc ON cmc.commit_id = c.id
            WHERE c.flake_id = $1
              AND (f3.snapshot_ready_at IS NULL OR fbcs.commit_id IS NOT NULL)
            ORDER BY
                CASE WHEN f3.snapshot_ready_at IS NOT NULL THEN 0 ELSE 1 END,
                CASE WHEN f3.snapshot_ready_at IS NOT NULL THEN fbcs.position ELSE 0 END ASC,
                c.commit_timestamp DESC,
                c.id DESC
            LIMIT $2
            "#,
        )
        .bind(flake_id)
        .bind(max_commits_per_flake)
        .fetch_all(pool)
        .await?;

        let commits: Vec<FlakeCommit> = commits_rows
            .into_iter()
            .map(|row| {
                let build_status = row.build_status.as_deref().map(|status| match status {
                    "queued" => BuildStatus::Queued,
                    "building" => BuildStatus::Building,
                    "failed" => BuildStatus::Failed,
                    "complete" => BuildStatus::Complete,
                    _ => BuildStatus::Idle,
                });

                let metadata = if let (
                    Some(total_systems),
                    Some(systems_passed_policy),
                    Some(systems_failed_policy_strict),
                    Some(systems_failed_policy_non_strict),
                    Some(has_nix_eval_error),
                    Some(has_policy_failures),
                    Some(all_systems_passed),
                ) = (
                    row.total_systems,
                    row.systems_passed_policy,
                    row.systems_failed_policy_strict,
                    row.systems_failed_policy_non_strict,
                    row.has_nix_eval_error,
                    row.has_policy_failures,
                    row.all_systems_passed,
                ) {
                    Some(CommitMetadata {
                        total_systems,
                        systems_passed_policy,
                        systems_failed_policy_strict,
                        systems_failed_policy_non_strict,
                        has_nix_eval_error,
                        has_policy_failures,
                        all_systems_passed,
                    })
                } else {
                    None
                };

                FlakeCommit {
                    id: row.id,
                    hash: row.git_commit_hash,
                    message: row.message.unwrap_or_default(),
                    author: row.author.unwrap_or_default(),
                    committed_at: row.commit_timestamp,
                    system_count: row.system_count,
                    commits_behind: row.commits_behind,
                    systems: row.systems,
                    system_paths: Vec::new(),
                    build_status,
                    evaluation_status: row.evaluation_status,
                    evaluation_error_message: row.evaluation_error_message,
                    metadata,
                }
            })
            .collect();

        timelines.push(FlakeTimeline {
            flake_id,
            flake_name,
            repo_url,
            commits,
        });
    }

    Ok(timelines)
}

// ---------------------------------------------------------------------------
// Branch-commit snapshot queries (TASK-397)
// ---------------------------------------------------------------------------

/// Atomically replace the branch-commit snapshot for a flake.
///
/// Deletes all existing snapshot rows for `flake_id`, inserts new rows from
/// `commits` (in order, where index 0 = HEAD), and sets `snapshot_ready_at`.
///
/// Must be called inside an open transaction so readers never see a partial
/// or empty snapshot. If `commits` is empty the snapshot is cleared but
/// `snapshot_ready_at` is still set (indicating an empty tracked branch
/// has been validated).
pub async fn replace_flake_branch_snapshot(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    flake_id: i32,
    commits: &[(i32, chrono::DateTime<chrono::Utc>)],
) -> Result<()> {
    // Delete existing snapshot rows for this flake
    sqlx::query("DELETE FROM flake_branch_commit_snapshot WHERE flake_id = $1")
        .bind(flake_id)
        .execute(&mut **tx)
        .await
        .context("Failed to delete old branch snapshot")?;

    // Insert new snapshot rows
    for (position, (commit_id, observed_at)) in commits.iter().enumerate() {
        let pos = position as i32;
        sqlx::query(
            r#"
            INSERT INTO flake_branch_commit_snapshot (flake_id, commit_id, position, observed_at)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (flake_id, commit_id) DO UPDATE
                SET position = EXCLUDED.position,
                    observed_at = EXCLUDED.observed_at
            "#,
        )
        .bind(flake_id)
        .bind(commit_id)
        .bind(pos)
        .bind(observed_at)
        .execute(&mut **tx)
        .await
        .context("Failed to insert branch snapshot row")?;
    }

    // Mark snapshot as ready
    sqlx::query(
        r#"
        UPDATE flakes
        SET snapshot_ready_at = COALESCE(snapshot_ready_at, now())
        WHERE id = $1
        "#,
    )
    .bind(flake_id)
    .execute(&mut **tx)
    .await
    .context("Failed to set snapshot_ready_at")?;

    Ok(())
}

/// Replace the branch snapshot for a flake using a connection (auto-transaction).
///
/// Opens its own short transaction. Prefer `replace_flake_branch_snapshot` when
/// the caller already has a transaction.
pub async fn replace_flake_branch_snapshot_standalone(
    pool: &PgPool,
    flake_id: i32,
    commits: &[(i32, chrono::DateTime<chrono::Utc>)],
) -> Result<()> {
    let mut tx = pool.begin().await?;
    replace_flake_branch_snapshot(&mut tx, flake_id, commits).await?;
    tx.commit().await?;
    Ok(())
}

/// Read the ordered branch snapshot for a flake.
///
/// Returns rows ordered by position ascending (position 0 = HEAD).
pub async fn read_flake_branch_snapshot(
    pool: &PgPool,
    flake_id: i32,
    limit: i64,
) -> Result<Vec<BranchCommitSnapshot>> {
    let rows = sqlx::query_as::<_, BranchCommitSnapshot>(
        r#"
        SELECT flake_id, commit_id, position, observed_at
        FROM flake_branch_commit_snapshot
        WHERE flake_id = $1
        ORDER BY position ASC
        LIMIT $2
        "#,
    )
    .bind(flake_id)
    .bind(limit)
    .fetch_all(pool)
    .await?;

    Ok(rows)
}

/// Check whether a flake has a ready branch snapshot.
///
/// Returns `true` if `snapshot_ready_at` is set (migration 0178 has populated
/// at least one snapshot for this flake).
pub async fn is_flake_snapshot_ready(pool: &PgPool, flake_id: i32) -> Result<bool> {
    let ready = sqlx::query_scalar::<_, Option<chrono::DateTime<chrono::Utc>>>(
        r#"
        SELECT snapshot_ready_at
        FROM flakes
        WHERE id = $1 AND deleted_at IS NULL
        "#,
    )
    .bind(flake_id)
    .fetch_optional(pool)
    .await?
    .flatten()
    .is_some();

    Ok(ready)
}
