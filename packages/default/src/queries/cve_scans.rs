use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};

/// Get the next target that needs CVE scanning (highest priority)
pub async fn get_next_target_needing_cve_scan(
    pool: &PgPool,
) -> Result<Option<EvaluationTargetWithScanInfo>> {
    let target = sqlx::query_as!(
        EvaluationTargetWithScanInfo,
        r#"
        SELECT 
            et.id,
            et.target_type,
            et.target_name,
            et.derivation_path,
            et.completed_at,
            et.status as evaluation_status,
            c.git_commit_hash,
            f.name as flake_name,
            f.repo_url,
            COALESCE(cs.attempts, 0) as scan_attempts,
            cs.status as scan_status,
            cs.scheduled_at as scan_scheduled_at
        FROM evaluation_targets et
        JOIN commits c ON et.commit_id = c.id
        JOIN flakes f ON c.flake_id = f.id
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
        LIMIT 1
        "#
    )
    .fetch_optional(pool)
    .await
    .context("Failed to fetch next target needing CVE scan")?;

    Ok(target)
}
