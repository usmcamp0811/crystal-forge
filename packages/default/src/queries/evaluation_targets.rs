use crate::models::commits::Commit;
use crate::models::evaluation_targets::{EvaluationTarget, TargetType};
use anyhow::Result;
use sqlx::PgPool;

pub async fn insert_evaluation_target(
    pool: &PgPool,
    commit: &Commit,
    target_name: &str,
    target_type: &str,
) -> Result<EvaluationTarget> {
    let inserted = sqlx::query_as!(
        EvaluationTarget,
        r#"
    INSERT INTO evaluation_targets (commit_id, target_type, target_name)
    VALUES ($1, $2, $3)
    ON CONFLICT (commit_id, target_type, target_name)
    DO UPDATE SET commit_id = EXCLUDED.commit_id
    RETURNING id, commit_id, target_type, target_name, derivation_path, build_timestamp, scheduled_at, completed_at, status
    "#,
        commit.id,
        target_type,
        target_name,
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
    UPDATE evaluation_targets
    SET derivation_path = $1,
        completed_at = now(),
        status = 'completed'
    WHERE commit_id = $2 AND target_type = $3 AND target_name = $4
    RETURNING
        id,
        commit_id,
        target_type as "target_type: TargetType",
        target_name,
        derivation_path,
        build_timestamp,
        scheduled_at,
        completed_at,
        status
    "#,
        path,
        target.commit_id,
        target.target_type.to_string(),
        target.target_name
    )
    .fetch_one(pool)
    .await?;

    Ok(updated)
}

pub async fn get_pending_targets(pool: &PgPool) -> Result<Vec<EvaluationTarget>> {
    let rows = sqlx::query_as!(
        EvaluationTarget,
        r#"
        SELECT
            id,
            commit_id,
            target_type as "target_type: TargetType",
            target_name,
            derivation_path,
            build_timestamp,
            scheduled_at,
            completed_at,
            status
        FROM evaluation_targets
        WHERE derivation_path IS NULL
        AND attempt_count <= 5
        ORDER BY scheduled_at DESC
        "#
    )
    .fetch_all(pool)
    .await?;

    Ok(rows)
}

pub async fn increment_evaluation_target_attempt_count(
    pool: &PgPool,
    target: &EvaluationTarget,
) -> Result<()> {
    let updated = sqlx::query!(
        r#"
    UPDATE evaluation_targets
    SET attempt_count = attempt_count + 1
    WHERE id = $1
    "#,
        target.id
    )
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn mark_target_in_progress(pool: &PgPool, target_id: i32) -> Result<()> {
    sqlx::query!(
        "UPDATE evaluation_targets SET status = 'in-progress' WHERE id = $1",
        target_id
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn reset_non_complete_targets(pool: &PgPool) -> Result<()> {
    sqlx::query!("UPDATE evaluation_targets SET status = 'pending' WHERE status != 'complete'")
        .execute(pool)
        .await?;
    Ok(())
}
