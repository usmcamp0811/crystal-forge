use crate::models::commits::Commit;
use crate::models::evaluation_targets::{EvaluationTarget, TargetType};
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use tracing::error;

/// Get targets that need CVE scanning (ordered by priority)
/// Returns array of EvaluationTargets - additional info can be fetched separately
pub async fn get_targets_needing_cve_scan(
    pool: &PgPool,
    limit: Option<i64>,
) -> Result<Vec<EvaluationTarget>> {
    let limit_clause = limit.unwrap_or(50); // Default to 50 if no limit specified

    let targets = sqlx::query_as!(
        EvaluationTarget,
        r#"
        SELECT 
            et.id,
            et.commit_id,
            et.target_type as "target_type: TargetType",
            et.target_name,
            et.derivation_path,
            et.scheduled_at,
            et.completed_at,
            et.status
        FROM evaluation_targets et
        LEFT JOIN cve_scans cs ON et.id = cs.evaluation_target_id
        WHERE 
            et.completed_at IS NOT NULL 
            AND et.status = 'complete'
            AND et.derivation_path IS NOT NULL
            AND (
                cs.id IS NULL 
                OR (cs.status IN ('failed', 'pending') AND cs.attempts < 5)
            )
        ORDER BY 
            COALESCE(cs.attempts, 0) ASC,
            et.completed_at ASC
        LIMIT $1
        "#,
        limit_clause
    )
    .fetch_all(pool)
    .await
    .context("Failed to fetch targets needing CVE scan")?;

    Ok(targets)
}
