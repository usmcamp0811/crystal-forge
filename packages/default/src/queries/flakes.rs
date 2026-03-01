use crate::api::models::{BuildStatus, FlakeCommit, FlakeRegistryItem, FlakeTimeline};
use crate::config::{FlakeConfig, WatchedFlake};
use crate::models::flakes::Flake;
use anyhow::Context;
use anyhow::Result;
use sqlx::PgPool;

pub async fn insert_flake(
    pool: &PgPool,
    name: &str,
    repo_url: &str,
    branch: &str,
) -> Result<Flake> {
    let flake = sqlx::query_as::<_, Flake>(
        "
        INSERT INTO flakes (name, repo_url, branch)
        VALUES ($1, $2, $3)
        ON CONFLICT (repo_url) DO UPDATE SET name = EXCLUDED.name, branch = EXCLUDED.branch
        RETURNING *
        ",
    )
    .bind(name)
    .bind(repo_url)
    .bind(branch)
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

pub async fn update_flake(
    pool: &PgPool,
    flake_id: i32,
    name: &str,
    repo_url: &str,
    branch: &str,
) -> Result<Flake> {
    let flake = sqlx::query_as::<_, Flake>(
        r#"
        UPDATE flakes
        SET name = $1,
            repo_url = $2,
            branch = $3
        WHERE id = $4
        RETURNING *
        "#,
    )
    .bind(name)
    .bind(repo_url)
    .bind(branch)
    .bind(flake_id)
    .fetch_one(pool)
    .await?;

    Ok(flake)
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
    let rows = sqlx::query!("SELECT name, repo_url, branch FROM flakes")
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
                branch: Some(row.branch),
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
        SELECT id, name, repo_url, branch
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

pub async fn list_flake_registry(pool: &PgPool) -> Result<Vec<FlakeRegistryItem>> {
    let rows = sqlx::query_as::<_, (i32, String, String, String, i64)>(
        r#"
        SELECT
            f.id,
            f.name,
            f.repo_url,
            f.branch,
            COUNT(s.id)::bigint AS system_count
        FROM flakes f
        LEFT JOIN systems s ON s.flake_id = f.id
        GROUP BY f.id, f.name, f.repo_url, f.branch
        ORDER BY lower(f.name) ASC
        "#,
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(
            |(id, name, repo_url, branch, system_count)| FlakeRegistryItem {
                id,
                name,
                repo_url,
                branch,
                system_count,
            },
        )
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

/// Fetch flake timelines for dashboard view (CF system deployment counts).
///
/// Returns up to `max_commits_per_flake` most recent commits for each flake,
/// showing count of Crystal Forge systems deployed at each commit.
pub async fn fetch_dashboard_flake_timelines(
    pool: &PgPool,
    max_commits_per_flake: i64,
) -> Result<Vec<FlakeTimeline>> {
    let flakes = sqlx::query_as::<_, (i32, String, String)>(
        "SELECT id, name, repo_url FROM flakes ORDER BY name ASC",
    )
    .fetch_all(pool)
    .await?;

    let mut timelines = Vec::new();

    for (flake_id, flake_name, repo_url) in flakes {
        let commits_rows = sqlx::query_as::<
            _,
            (
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
            .map(|(hash, committed_at, system_count, systems, commits_behind, build_status, evaluation_status)| {
                let build_status = build_status.as_deref().map(|status| match status {
                    "queued" => BuildStatus::Queued,
                    "building" => BuildStatus::Building,
                    "failed" => BuildStatus::Failed,
                    "complete" => BuildStatus::Complete,
                    _ => BuildStatus::Idle,
                });

                FlakeCommit {
                hash,
                message: "".to_string(),
                author: "".to_string(),
                committed_at,
                system_count,
                commits_behind,
                systems,
                build_status,
                evaluation_status,
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

/// Fetch flake timelines for flakes view (nixosConfigurations in flake).
///
/// Returns up to `max_commits_per_flake` most recent commits for each flake,
/// showing nixosConfigurations discovered at each commit from cache.
pub async fn fetch_flake_timelines(
    pool: &PgPool,
    max_commits_per_flake: i64,
) -> Result<Vec<FlakeTimeline>> {
    // First, get all flakes
    let flakes = sqlx::query_as::<_, (i32, String, String)>(
        "SELECT id, name, repo_url FROM flakes ORDER BY name ASC",
    )
    .fetch_all(pool)
    .await?;

    let mut timelines = Vec::new();

    for (flake_id, flake_name, repo_url) in flakes {
        // Fetch recent commits for this flake, including systems at commit,
        // build queue status, dry-run/eval status, and git metadata (message/author).
        let commits_rows = sqlx::query_as::<
            _,
            (
                String,
                chrono::DateTime<chrono::Utc>,
                Option<String>,
                Option<String>,
                i64,
                Vec<String>,
                i64,
                Option<String>,
                Option<String>,
            ),
        >(
            r#"
            SELECT
                c.git_commit_hash,
                c.commit_timestamp,
                c.message,
                c.author,
                COALESCE(CARDINALITY(cac.nixos_configurations), 0)::bigint AS system_count,
                COALESCE(cac.nixos_configurations, ARRAY[]::text[]) AS systems,
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
            LEFT JOIN commit_artifacts_cache cac ON cac.commit_id = c.id
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
            .map(|(hash, committed_at, message, author, system_count, systems, commits_behind, build_status, evaluation_status)| {
                let build_status = build_status.as_deref().map(|status| match status {
                    "queued" => BuildStatus::Queued,
                    "building" => BuildStatus::Building,
                    "failed" => BuildStatus::Failed,
                    "complete" => BuildStatus::Complete,
                    _ => BuildStatus::Idle,
                });

                FlakeCommit {
                hash,
                message: message.unwrap_or_default(),
                author: author.unwrap_or_default(),
                committed_at,
                system_count,
                commits_behind,
                systems,
                build_status,
                evaluation_status,
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
