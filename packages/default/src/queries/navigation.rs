//! Navigation badge aggregate queries.
//!
//! Provides cheap COUNT queries for the sidebar badge counts so the UI can show
//! "needs attention" signals per navigation entry.  Each count reuses the same
//! semantics as its corresponding list endpoint (e.g. system health = same view
//! that drives the Systems list).

use crate::api::models::NavigationBadges;
use anyhow::Result;
use sqlx::PgPool;

/// Fetch all sidebar badge counts in one round-trip.
///
/// Each sub-count is a separate scalar query; the implementation deliberately
/// avoids a complex JOIN so each count stays cheap and independently correct.
pub async fn fetch_navigation_badges(pool: &PgPool) -> Result<NavigationBadges> {
    // ── Systems: critical or offline ──────────────────────────────────────────
    // Mirrors the health_status column from view_system_deployment_status which
    // the Systems list already uses.
    let (systems_attention, systems_total): (i64, i64) = sqlx::query_as(
        r#"
        SELECT
            COUNT(*) FILTER (WHERE health_status IN ('critical', 'offline'))::bigint,
            COUNT(*)::bigint
        FROM view_system_deployment_status
        WHERE is_active = true
        "#,
    )
    .fetch_one(pool)
    .await?;

    // ── Flakes: sync_status = error ───────────────────────────────────────────
    let (flakes_errored, flakes_total): (i64, i64) = sqlx::query_as(
        r#"
        SELECT
            COUNT(*) FILTER (WHERE sync_status = 'error')::bigint,
            COUNT(*)::bigint
        FROM flakes
        WHERE deleted_at IS NULL
        "#,
    )
    .fetch_one(pool)
    .await?;

    // ── Environments: contain ≥1 attention system ─────────────────────────────
    // Uses the environment rollup view which the Environments list uses.
    let (environments_attention, environments_total): (i64, i64) = sqlx::query_as(
        r#"
        SELECT
            COUNT(*) FILTER (WHERE critical_count > 0 OR offline_count > 0)::bigint,
            COUNT(*)::bigint
        FROM view_environment_rollups
        "#,
    )
    .fetch_one(pool)
    .await?;

    // ── Builds: failed derivations in the last 24h ────────────────────────────
    let builds_failed_24h: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*)::bigint
        FROM derivations
        WHERE status = 'failed'
          AND updated_at >= now() - interval '24 hours'
        "#,
    )
    .fetch_one(pool)
    .await?;

    // ── Evals: failed commit evaluations in the last 24h ─────────────────────
    let evals_failed_24h: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*)::bigint
        FROM commits
        WHERE evaluation_status = 'failed'
          AND updated_at >= now() - interval '24 hours'
        "#,
    )
    .fetch_one(pool)
    .await?;

    // ── CVEs: critical open across the fleet ─────────────────────────────────
    // Reuses the same view as /api/v1/cves/stats (view_cve_fleet_stats).
    let cves_critical: i64 = sqlx::query_scalar(
        r#"
        SELECT COALESCE((to_jsonb(v)->>'critical')::bigint, 0)
        FROM view_cve_fleet_stats v
        LIMIT 1
        "#,
    )
    .fetch_optional(pool)
    .await?
    .unwrap_or(0);

    Ok(NavigationBadges {
        systems_attention,
        systems_total,
        flakes_errored,
        flakes_total,
        environments_attention,
        environments_total,
        builds_failed_24h,
        evals_failed_24h,
        cves_critical,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn test_fetch_navigation_badges_returns_without_error() {
        let pool = sqlx::PgPool::connect(
            &std::env::var("DATABASE_URL").expect("DATABASE_URL must be set for DB tests"),
        )
        .await
        .expect("Failed to connect");

        let badges = fetch_navigation_badges(&pool)
            .await
            .expect("fetch_navigation_badges failed");

        // Totals must be non-negative and attention ≤ total
        assert!(badges.systems_total >= 0);
        assert!(badges.systems_attention <= badges.systems_total);
        assert!(badges.flakes_total >= 0);
        assert!(badges.flakes_errored <= badges.flakes_total);
        assert!(badges.environments_total >= 0);
        assert!(badges.environments_attention <= badges.environments_total);
        assert!(badges.builds_failed_24h >= 0);
        assert!(badges.evals_failed_24h >= 0);
        assert!(badges.cves_critical >= 0);
    }
}
