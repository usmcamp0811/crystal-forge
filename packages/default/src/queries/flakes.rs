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

/// Fetch flake timelines with recent commits for the dashboard.
///
/// Returns up to `max_commits_per_flake` most recent commits for each flake,
/// including system count and commits-behind calculation.
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
        // Get the latest commit timestamp for this flake to calculate commits_behind
        let latest_commit_ts = sqlx::query_scalar::<_, Option<chrono::DateTime<chrono::Utc>>>(
            "SELECT commit_timestamp FROM commits WHERE flake_id = $1 ORDER BY commit_timestamp DESC LIMIT 1"
        )
        .bind(flake_id)
        .fetch_optional(pool)
        .await?
        .flatten();

        // Fetch recent commits for this flake
        // TODO: Add system counts when we have proper commit->system tracking
        let commits_rows = sqlx::query!(
            r#"
            SELECT 
                c.git_commit_hash,
                c.commit_timestamp,
                0::bigint as system_count,
                ARRAY[]::text[] as "systems!",
                (
                    SELECT COUNT(*)::bigint
                    FROM commits c2
                    WHERE c2.flake_id = c.flake_id
                    AND c2.commit_timestamp > c.commit_timestamp
                ) as "commits_behind!"
            FROM commits c
            WHERE c.flake_id = $1
            GROUP BY c.id, c.git_commit_hash, c.commit_timestamp, c.flake_id
            ORDER BY c.commit_timestamp DESC
            LIMIT $2
            "#,
            flake_id,
            max_commits_per_flake
        )
        .fetch_all(pool)
        .await?;

        let commits: Vec<FlakeCommit> = commits_rows
            .into_iter()
            .map(|row| FlakeCommit {
                hash: row.git_commit_hash,
                message: "".to_string(), // We don't store commit messages in the database
                author: "".to_string(),  // We don't store commit authors in the database
                committed_at: row.commit_timestamp,
                system_count: row.system_count.unwrap_or(0),
                commits_behind: row.commits_behind,
                systems: row.systems,
                build_status: Some(BuildStatus::Idle), // TODO: Query actual build status from derivations
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
