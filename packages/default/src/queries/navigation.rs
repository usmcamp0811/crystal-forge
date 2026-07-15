//! Navigation badge aggregate queries.
//!
//! Provides cheap COUNT queries for the sidebar badge counts so the UI can show
//! "needs attention" signals per navigation entry. Each count reuses the same
//! semantics as its corresponding list endpoint (e.g. system health = same view
//! that drives the Systems list).
//!
//! Counts are computed as "new since the user's last acknowledgment" of that
//! category (see `user_alert_acknowledgments`), not raw totals. Without this,
//! a badge showing e.g. "25 failed builds" reappears identically on every
//! page refresh even after the user has already looked, training users to
//! ignore it. Categories with a discrete per-item completion/detection
//! timestamp (flakes, builds, evals, cves) compute a true delta: items whose
//! timestamp is newer than the user's last acknowledgment. Categories without
//! one (systems, environments — health status is a continuously-recomputed
//! function of heartbeat staleness, not a discrete event) instead show the
//! current total only when it differs from what was recorded at last
//! acknowledgment, avoiding a misleading subtraction.

use crate::api::models::NavigationBadges;
use anyhow::Result;
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use std::collections::HashMap;
use uuid::Uuid;

/// Capture `NOW()` once at the start of a badge fetch so all sub-queries use
/// a consistent observation timestamp and the value can be returned to the
/// client as `observed_at`.
async fn capture_now(pool: &PgPool) -> DateTime<Utc> {
    sqlx::query_scalar::<_, DateTime<Utc>>("SELECT NOW()")
        .fetch_one(pool)
        .await
        .unwrap_or_else(|_| Utc::now())
}

/// One row of `user_alert_acknowledgments`.
#[derive(Debug, Clone)]
struct AckBaseline {
    last_seen_at: DateTime<Utc>,
    last_seen_count: i64,
    /// MD5 of the sorted alerting-ID set at acknowledgment time
    /// (systems/environments only). `None` if this row was written before the
    /// fingerprint column was added.
    last_seen_fingerprint: Option<String>,
}

/// Fetch a user's acknowledgment baseline for every category in one query.
async fn fetch_user_acknowledgments(
    pool: &PgPool,
    user_id: Uuid,
) -> Result<HashMap<String, AckBaseline>> {
    let rows: Vec<(String, DateTime<Utc>, i64, Option<String>)> = sqlx::query_as(
        r#"
        SELECT category, last_seen_at, last_seen_count, last_seen_fingerprint
        FROM user_alert_acknowledgments
        WHERE user_id = $1
        "#,
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(
            |(category, last_seen_at, last_seen_count, last_seen_fingerprint)| {
                (
                    category,
                    AckBaseline {
                        last_seen_at,
                        last_seen_count,
                        last_seen_fingerprint,
                    },
                )
            },
        )
        .collect())
}

/// Record a user's acknowledgment of a category's current state.
///
/// `observed_at` must be the `observed_at` cursor from the badge response the
/// user was actually shown. Using the client-provided cursor rather than
/// `NOW()` at POST time prevents a failure that arrived between the badge fetch
/// and the acknowledge POST from being silently consumed — it will still appear
/// in the next badge response because its event timestamp is newer than the
/// anchored `observed_at`.
///
/// `current_count` is the raw attention count at the moment of acknowledgment
/// (used as the baseline for count-diff categories like systems/environments;
/// timestamp categories use `observed_at` as the cutoff going forward).
pub async fn upsert_user_alert_acknowledgment(
    pool: &PgPool,
    user_id: Uuid,
    category: &str,
    observed_at: DateTime<Utc>,
    current_count: i64,
    fingerprint: Option<&str>,
) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO user_alert_acknowledgments
            (user_id, category, last_seen_at, last_seen_count, last_seen_fingerprint, updated_at)
        VALUES ($1, $2, $3, $4, $5, NOW())
        ON CONFLICT (user_id, category) DO UPDATE
        SET last_seen_at = EXCLUDED.last_seen_at,
            last_seen_count = EXCLUDED.last_seen_count,
            last_seen_fingerprint = EXCLUDED.last_seen_fingerprint,
            updated_at = NOW()
        WHERE EXCLUDED.last_seen_at >= user_alert_acknowledgments.last_seen_at
        "#,
    )
    .bind(user_id)
    .bind(category)
    .bind(observed_at)
    .bind(current_count)
    .bind(fingerprint)
    .execute(pool)
    .await?;

    Ok(())
}

/// For count-diff categories (systems/environments — no discrete per-item
/// event timestamp): show the current total only if either the count changed
/// OR the set of alerting IDs changed (fingerprint mismatch), so a
/// replacement failure (system A recovers while system B goes critical, same
/// count) still surfaces as new.
///
/// The fingerprint is an MD5 hash of the sorted newline-joined alerting ID
/// strings, computed by the caller before this function. If the baseline has
/// no fingerprint (row written before migration 0161), falls back to count-only
/// comparison.
fn new_since_by_count(
    current_total: i64,
    current_fingerprint: Option<&str>,
    baseline: Option<&AckBaseline>,
) -> i64 {
    match baseline {
        None => current_total,
        Some(b) => {
            // Fingerprint comparison takes precedence when available on both sides.
            let fingerprint_changed =
                match (current_fingerprint, b.last_seen_fingerprint.as_deref()) {
                    (Some(cur), Some(prev)) => cur != prev,
                    _ => false, // one or both absent: fall through to count comparison
                };
            if fingerprint_changed || b.last_seen_count != current_total {
                current_total
            } else {
                0
            }
        }
    }
}

/// Fetch all sidebar badge counts in one round-trip, computed relative to
/// `user_id`'s last acknowledgment of each category.
///
/// Each sub-count is a separate scalar query; the implementation deliberately
/// avoids a complex JOIN so each count stays cheap and independently correct.
pub async fn fetch_navigation_badges(
    pool: &PgPool,
    user_id: Uuid,
    is_admin: bool,
    member_environment_ids: &[Uuid],
) -> Result<NavigationBadges> {
    // Capture the observation timestamp once so all sub-queries share a
    // consistent cursor and the client can echo it back on acknowledge.
    let observed_at = capture_now(pool).await;

    let acks = fetch_user_acknowledgments(pool, user_id)
        .await
        .unwrap_or_default();

    // ── Systems: critical or offline ──────────────────────────────────────────
    // Mirrors the health_status column from view_system_list (already
    // filtered to active systems) which the Systems list uses. Scoped to the
    // requesting user's environment memberships — admins see the fleet-wide
    // total, non-admin operators/viewers only see systems in environments
    // they belong to, matching GET /api/v1/systems's visibility rule
    // (Role::can_access_system_environment). No discrete "became critical at"
    // timestamp exists (health is derived from heartbeat staleness), so this
    // uses the fingerprint+count-diff baseline. The fingerprint is an MD5 of
    // the sorted set of alerting system IDs so replacement failures (A
    // recovers while B goes critical — same count, different set) re-surface.
    let (systems_attention_total, systems_total, systems_fingerprint): (i64, i64, Option<String>) =
        match sqlx::query_as(
            r#"
        SELECT
            COUNT(*) FILTER (WHERE vsl.health_status IN ('critical', 'offline'))::bigint,
            COUNT(*)::bigint,
            NULLIF(
                md5(string_agg(
                    vsl.id::text,
                    E'\n'
                    ORDER BY vsl.id
                ) FILTER (WHERE vsl.health_status IN ('critical', 'offline'))),
                ''
            )
        FROM view_system_list vsl
        JOIN systems s ON s.id = vsl.id
        WHERE $1 OR s.environment_id = ANY($2)
        "#,
        )
        .bind(is_admin)
        .bind(member_environment_ids)
        .fetch_one(pool)
        .await
        {
            Ok(counts) => counts,
            Err(e) => {
                tracing::warn!("Failed to fetch systems navigation badge counts: {e:#}");
                (0, 0, None)
            }
        };
    let systems_attention = new_since_by_count(
        systems_attention_total,
        systems_fingerprint.as_deref(),
        acks.get("systems"),
    );

    // ── Flakes: sync_status = error (or stale 'syncing'), new since
    // last_sync_at baseline ───────────────────────────────────────────────
    // Mirrors the effective-status predicate in queries::flakes::
    // list_flake_registry: a flake stuck in 'syncing' for >30 minutes with no
    // completion is treated as errored there (surfaced with a "Sync appears
    // stale" message), so the badge count must use the same predicate or it
    // will silently under-count relative to what /flakes actually shows.
    let flakes_since = acks.get("flakes").map(|b| b.last_seen_at);
    let (flakes_errored, flakes_total): (i64, i64) = match sqlx::query_as(
        r#"
        SELECT
            COUNT(*) FILTER (
                WHERE
                    -- Explicit sync error: transition time is last_sync_at.
                    (
                        sync_status = 'error'
                        AND ($1::timestamptz IS NULL OR last_sync_at > $1)
                        AND last_sync_at <= $2
                    )
                    OR
                    -- Stale syncing (>30 min): effective error onset is
                    -- last_sync_at + 30 minutes, not last_sync_at itself.
                    -- A sync that was already in-flight when the user
                    -- acknowledged must still resurface once it crosses the
                    -- staleness threshold.
                    (
                        sync_status = 'syncing'
                        AND last_sync_at IS NOT NULL
                        AND last_sync_at < now() - interval '30 minutes'
                        AND ($1::timestamptz IS NULL
                             OR last_sync_at + interval '30 minutes' > $1)
                        AND last_sync_at + interval '30 minutes' <= $2
                    )
            )::bigint,
            COUNT(*)::bigint
        FROM flakes
        WHERE deleted_at IS NULL
        "#,
    )
    .bind(flakes_since)
    .bind(observed_at)
    .fetch_one(pool)
    .await
    {
        Ok(counts) => counts,
        Err(e) => {
            tracing::warn!("Failed to fetch flakes navigation badge counts: {e:#}");
            (0, 0)
        }
    };

    // ── Environments: contain ≥1 attention system (fingerprint+count-diff) ───
    // Scoped to the requesting user's environment memberships — admins see
    // every environment, non-admin operators/viewers only see environments
    // they belong to, matching GET /api/v1/environments's visibility rule
    // (list_environments_for_user / user_environment_memberships).
    // Fingerprint guards against replacement failures (env A clears while env
    // B becomes critical — same total, different set).
    let (environments_attention_total, environments_total, environments_fingerprint): (
        i64,
        i64,
        Option<String>,
    ) = match sqlx::query_as(
        r#"
        SELECT
            COUNT(*) FILTER (WHERE critical_count > 0 OR offline_count > 0)::bigint,
            COUNT(*)::bigint,
            NULLIF(
                md5(string_agg(
                    environment_id::text,
                    E'\n'
                    ORDER BY environment_id
                ) FILTER (WHERE critical_count > 0 OR offline_count > 0)),
                ''
            )
        FROM view_environment_rollups
        WHERE $1 OR environment_id = ANY($2)
        "#,
    )
    .bind(is_admin)
    .bind(member_environment_ids)
    .fetch_one(pool)
    .await
    {
        Ok(counts) => counts,
        Err(e) => {
            tracing::warn!("Failed to fetch environments navigation badge counts: {e:#}");
            (0, 0, None)
        }
    };
    let environments_attention = new_since_by_count(
        environments_attention_total,
        environments_fingerprint.as_deref(),
        acks.get("environments"),
    );

    // ── Builds: failed build jobs, new since completed_at baseline ───────────
    let builds_since = acks.get("builds").map(|b| b.last_seen_at);
    let builds_failed_new: i64 = match sqlx::query_scalar(
        r#"
        SELECT COUNT(*)::bigint
        FROM build_jobs
        WHERE status = 'failed'
          AND ($1::timestamptz IS NULL OR completed_at > $1)
          AND completed_at <= $2
        "#,
    )
    .bind(builds_since)
    .bind(observed_at)
    .fetch_one(pool)
    .await
    {
        Ok(count) => count,
        Err(e) => {
            tracing::warn!("Failed to fetch builds navigation badge count: {e:#}");
            0
        }
    };

    // ── Evals: failed commit evaluations, new since evaluation_completed_at ──
    let evals_since = acks.get("evals").map(|b| b.last_seen_at);
    let evals_failed_new: i64 = match sqlx::query_scalar(
        r#"
        SELECT COUNT(*)::bigint
        FROM commits
        WHERE evaluation_status = 'failed'
          AND ($1::timestamptz IS NULL OR evaluation_completed_at > $1)
          AND evaluation_completed_at <= $2
        "#,
    )
    .bind(evals_since)
    .bind(observed_at)
    .fetch_one(pool)
    .await
    {
        Ok(count) => count,
        Err(e) => {
            tracing::warn!("Failed to fetch evaluations navigation badge count: {e:#}");
            0
        }
    };

    // ── CVEs: critical, new since first_seen baseline ────────────────────────
    // Reuses the same view + severity predicate as /api/v1/cves/stats
    // (view_cve_fleet_stats is itself computed from view_cve_list_with_metadata).
    let cves_since = acks.get("cves").map(|b| b.last_seen_at);
    let cves_critical_new: i64 = match sqlx::query_scalar(
        r#"
        SELECT COUNT(*)::bigint
        FROM view_cve_list_with_metadata
        WHERE severity = 'CRITICAL'
          AND ($1::timestamptz IS NULL OR first_seen > $1)
          AND first_seen <= $2
        "#,
    )
    .bind(cves_since)
    .bind(observed_at)
    .fetch_one(pool)
    .await
    {
        Ok(count) => count,
        Err(e) => {
            tracing::warn!("Failed to fetch CVE navigation badge count: {e:#}");
            0
        }
    };

    Ok(NavigationBadges {
        observed_at: Some(observed_at),
        systems_attention,
        systems_total,
        systems_fingerprint,
        flakes_errored,
        flakes_total,
        environments_attention,
        environments_total,
        environments_fingerprint,
        builds_failed_new,
        evals_failed_new,
        cves_critical_new,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_since_by_count_shows_full_total_when_never_acknowledged() {
        assert_eq!(new_since_by_count(7, None, None), 7);
    }

    #[test]
    fn new_since_by_count_hides_when_unchanged_since_acknowledgment() {
        let baseline = AckBaseline {
            last_seen_at: Utc::now(),
            last_seen_count: 7,
            last_seen_fingerprint: None,
        };
        assert_eq!(new_since_by_count(7, None, Some(&baseline)), 0);
    }

    #[test]
    fn new_since_by_count_shows_total_when_count_changed_since_acknowledgment() {
        let baseline = AckBaseline {
            last_seen_at: Utc::now(),
            last_seen_count: 7,
            last_seen_fingerprint: None,
        };
        // Any change from the acknowledged baseline (up or down) re-surfaces
        // the current total rather than fabricating a delta, since count-diff
        // categories have no discrete per-item timestamp to compute a true
        // "new" count from.
        assert_eq!(new_since_by_count(9, None, Some(&baseline)), 9);
        assert_eq!(new_since_by_count(3, None, Some(&baseline)), 3);
    }

    #[test]
    fn new_since_by_count_shows_total_when_fingerprint_changed() {
        let baseline = AckBaseline {
            last_seen_at: Utc::now(),
            last_seen_count: 1,
            last_seen_fingerprint: Some("abc123".to_string()),
        };
        // Same total, different fingerprint (replacement failure): must resurface.
        assert_eq!(new_since_by_count(1, Some("def456"), Some(&baseline)), 1);
    }

    #[test]
    fn new_since_by_count_hides_when_fingerprint_and_count_match() {
        let baseline = AckBaseline {
            last_seen_at: Utc::now(),
            last_seen_count: 1,
            last_seen_fingerprint: Some("abc123".to_string()),
        };
        assert_eq!(new_since_by_count(1, Some("abc123"), Some(&baseline)), 0);
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn test_fetch_navigation_badges_returns_without_error() {
        let pool = sqlx::PgPool::connect(
            &std::env::var("DATABASE_URL").expect("DATABASE_URL must be set for DB tests"),
        )
        .await
        .expect("Failed to connect");

        let user_id = uuid::Uuid::new_v4();
        // is_admin=true exercises the unscoped fleet-wide path without
        // requiring a real users/user_environment_memberships fixture row.
        let badges = fetch_navigation_badges(&pool, user_id, true, &[])
            .await
            .expect("fetch_navigation_badges failed");

        // Totals must be non-negative and attention <= total
        assert!(badges.systems_total >= 0);
        assert!(badges.systems_attention <= badges.systems_total);
        assert!(badges.flakes_total >= 0);
        assert!(badges.flakes_errored <= badges.flakes_total);
        assert!(badges.environments_total >= 0);
        assert!(badges.environments_attention <= badges.environments_total);
        assert!(badges.builds_failed_new >= 0);
        assert!(badges.evals_failed_new >= 0);
        assert!(badges.cves_critical_new >= 0);
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn test_environments_total_scoped_to_user_membership() {
        // Regression test: fetch_navigation_badges previously counted
        // systems/environments fleet-wide regardless of the requesting
        // user's environment memberships, so a non-admin operator/viewer
        // could see an attention badge for environments they cannot
        // actually see in the Environments view (reported as "alert pill
        // on environments with a 2 but I don't see anything that would
        // cause it").
        let pool = sqlx::PgPool::connect(
            &std::env::var("DATABASE_URL").expect("DATABASE_URL must be set for DB tests"),
        )
        .await
        .expect("Failed to connect");

        let user_id = uuid::Uuid::new_v4();
        let short_id = user_id.simple().to_string()[..12].to_string();
        sqlx::query(
            "INSERT INTO users (id, username, first_name, last_name, email, user_type) VALUES ($1, $2, 'Test', 'User', $3, 'human')",
        )
        .bind(user_id)
        .bind(format!("nst-{short_id}"))
        .bind(format!("nst-{short_id}@example.com"))
        .execute(&pool)
        .await
        .expect("failed to insert throwaway test user");

        let member_env_id = uuid::Uuid::new_v4();
        let other_env_id = uuid::Uuid::new_v4();
        let member_short = member_env_id.simple().to_string()[..12].to_string();
        let other_short = other_env_id.simple().to_string()[..12].to_string();
        sqlx::query("INSERT INTO environments (id, name) VALUES ($1, $2), ($3, $4)")
            .bind(member_env_id)
            .bind(format!("nsm-{member_short}"))
            .bind(other_env_id)
            .bind(format!("nso-{other_short}"))
            .execute(&pool)
            .await
            .expect("failed to insert throwaway test environments");

        sqlx::query(
            "INSERT INTO user_environment_memberships (user_id, environment_id) VALUES ($1, $2)",
        )
        .bind(user_id)
        .bind(member_env_id)
        .execute(&pool)
        .await
        .expect("failed to insert throwaway membership");

        // Non-admin, scoped to only member_env_id: environments_total must
        // not include other_env_id.
        let scoped = fetch_navigation_badges(&pool, user_id, false, &[member_env_id])
            .await
            .expect("scoped fetch_navigation_badges failed");
        let scoped_before = scoped.environments_total;

        // Admin (or a member of both): environments_total must be at least
        // 2 higher than the single-environment scoped view, proving the
        // WHERE $1 OR environment_id = ANY($2) predicate actually narrows
        // results rather than being a no-op.
        let admin = fetch_navigation_badges(&pool, user_id, true, &[])
            .await
            .expect("admin fetch_navigation_badges failed");

        assert!(
            admin.environments_total >= scoped_before + 1,
            "admin total ({}) should include at least the extra unscoped environment beyond the member-scoped total ({})",
            admin.environments_total,
            scoped_before
        );

        // Cleanup (cascades memberships via ON DELETE CASCADE).
        sqlx::query("DELETE FROM environments WHERE id = ANY($1)")
            .bind([member_env_id, other_env_id])
            .execute(&pool)
            .await
            .ok();
        sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(user_id)
            .execute(&pool)
            .await
            .ok();
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn test_acknowledgment_upsert_and_roundtrip() {
        let pool = sqlx::PgPool::connect(
            &std::env::var("DATABASE_URL").expect("DATABASE_URL must be set for DB tests"),
        )
        .await
        .expect("Failed to connect");

        // Self-contained: create a throwaway user (FK requirement), exercise
        // upsert + roundtrip + conflict-update, then clean up.
        let user_id = uuid::Uuid::new_v4();
        sqlx::query(
            r#"
            INSERT INTO users (id, username, first_name, last_name, email, user_type)
            VALUES ($1, $2, 'Test', 'User', $3, 'human')
            "#,
        )
        .bind(user_id)
        .bind(format!("nav-test-{user_id}"))
        .bind(format!("nav-test-{user_id}@example.com"))
        .execute(&pool)
        .await
        .expect("failed to insert throwaway test user");

        upsert_user_alert_acknowledgment(&pool, user_id, "builds", Utc::now(), 5, None)
            .await
            .expect("initial upsert should succeed");

        let acks = fetch_user_acknowledgments(&pool, user_id)
            .await
            .expect("fetch should succeed");
        let builds_ack = acks.get("builds").expect("builds ack should exist");
        assert_eq!(builds_ack.last_seen_count, 5);

        // Conflict path: re-acknowledging updates the existing row rather
        // than erroring or duplicating.
        upsert_user_alert_acknowledgment(&pool, user_id, "builds", Utc::now(), 9, None)
            .await
            .expect("conflict upsert should succeed");
        let acks = fetch_user_acknowledgments(&pool, user_id)
            .await
            .expect("fetch should succeed");
        assert_eq!(acks.get("builds").unwrap().last_seen_count, 9);
        assert_eq!(acks.len(), 1, "only one row per (user, category)");

        // Cleanup.
        sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(user_id)
            .execute(&pool)
            .await
            .ok();
    }
}
