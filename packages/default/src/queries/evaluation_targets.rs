use crate::evaluation_targets::EvaluationTarget;
use crate::pending_targets::PendingTarget;
use anyhow::{Context, Result};
use futures::TryStreamExt;
use futures::stream::Stream;
use sqlx::{PgPool, Row};

pub async fn insert_evaluation_target(
    pool: &PgPool,
    commit: &Commit,
    target_name: &str,
    target_type: &str,
) -> Result<EvaluationTarget> {
    let inserted = sqlx::query_as!(
        EvaluationTarget,
        r#"
        INSERT INTO tbl_evaluation_targets (commit_id, target_type, target_name)
        VALUES ($1, $2, $3)
        ON CONFLICT (commit_id, target_type, target_name) DO UPDATE SET commit_id = EXCLUDED.commit_id
        RETURNING id, commit_id, target_type, target_name, derivation_path, build_timestamp
        "#,
        commit.id,
        target_type,
        target_name
    )
    .fetch_one(pool)
    .await?;

    Ok(inserted)
}

pub async fn update_evaluation_target_path(
    pool: &PgPool,
    target: &EvaluationTarget,
    path: &str,
) -> Result<EvaluationTarget> {
    let updated = sqlx::query_as!(
        EvaluationTarget,
        r#"
        UPDATE tbl_evaluation_targets
        SET derivation_path = $1
        WHERE commit_id = $2 AND target_type = $3 AND target_name = $4
        RETURNING id, commit_id, target_type, target_name, derivation_path, build_timestamp
        "#,
        path,
        target.commit_id,
        target.target_type,
        target.target_name
    )
    .fetch_one(pool)
    .await?;

    Ok(updated)
}

pub async fn list_pending_evaluation_targets(
    pool: &PgPool,
) -> Result<impl Stream<Item = Result<PendingTarget>>> {
    let stream = sqlx::query!(
        r#"
        SELECT f.name, f.repo_url, c.git_commit_hash, d.target_type
        FROM tbl_evaluation_targets d
        JOIN tbl_commits c ON d.commit_id = c.id
        JOIN tbl_flakes f ON c.flake_id = f.id
        WHERE d.derivation_path IS NULL
        "#
    )
    .fetch(pool)
    .map_ok(|r| PendingTarget {
        flake_name: r.name,
        repo_url: r.repo_url,
        commit_hash: r.git_commit_hash,
        target_type: r.target_type,
    });

    Ok(stream)
}

pub async fn get_pending_targets(pool: &PgPool) -> Result<Vec<EvaluationTarget>> {
    let rows = sqlx::query_as!(
        EvaluationTarget,
        r#"
        SELECT
            id,
            commit_id,
            type AS target_type,
            name AS target_name,
            hash AS derivation_path,
            build_timestamp
        FROM tbl_evaluation_targets
        WHERE hash IS NULL
        "#
    )
    .fetch_all(pool)
    .await?;

    Ok(rows)
}
