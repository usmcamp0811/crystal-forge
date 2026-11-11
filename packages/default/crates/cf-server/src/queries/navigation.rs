//! Navigation badge aggregate queries.
//!
//! Provides sidebar badge counts and the per-user dismissal contract.
//! Counts are now "eligible undismissed canonical occurrences" opened within
//! the last 24 hours, not mutable category baselines. The Systems total is the
//! active registration registry across all modes. Operational system/environment
//! alerts require an underlying active managed machine, including immediately
//! after a mode change while old occurrences await reconciliation.

use crate::api::models::NavigationBadges;
use crate::queries::attention;
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use sqlx::PgPool;
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

async fn total_systems(
    pool: &PgPool,
    is_admin: bool,
    member_environment_ids: &[Uuid],
) -> Result<i64> {
    sqlx::query_scalar(
        "SELECT COUNT(*)::bigint FROM systems WHERE is_active = TRUE AND ($1 OR environment_id = ANY($2))",
    )
    .bind(is_admin)
    .bind(member_environment_ids)
    .fetch_one(pool)
    .await
    .context("failed to fetch systems total")
}

async fn total_environments(
    pool: &PgPool,
    is_admin: bool,
    member_environment_ids: &[Uuid],
) -> Result<i64> {
    sqlx::query_scalar("SELECT COUNT(*)::bigint FROM environments WHERE ($1 OR id = ANY($2))")
        .bind(is_admin)
        .bind(member_environment_ids)
        .fetch_one(pool)
        .await
        .context("failed to fetch environments total")
}

async fn total_flakes(pool: &PgPool) -> Result<i64> {
    sqlx::query_scalar("SELECT COUNT(*)::bigint FROM flakes WHERE deleted_at IS NULL")
        .fetch_one(pool)
        .await
        .context("failed to fetch flakes total")
}

// SECURITY: Filter persisted occurrences at read time as well as in the health
// view. A mode change must not leave an old outage visible until reconciliation.
async fn operational_attention(
    pool: &PgPool,
    user_id: Uuid,
    category: &str,
    observed_at: DateTime<Utc>,
    is_admin: bool,
    environment_ids: &[Uuid],
) -> Result<(i64, Vec<String>)> {
    let (total, keys): (i64, Option<Vec<String>>) = sqlx::query_as(
        r#"WITH eligible AS (
          SELECT ao.source_occurrence_key, ao.opened_at, ao.id
          FROM attention_occurrences ao
          JOIN systems s ON s.id::text = CASE WHEN $2='systems' THEN ao.subject_id
            ELSE ao.metadata->>'underlying_system_id' END
          WHERE ao.category=$2 AND ao.resolved_at IS NULL
            AND s.is_active AND s.management_mode='managed'
            AND ao.opened_at >= $3 - INTERVAL '24 hours' AND ao.opened_at <= $3
            AND ($4 OR s.environment_id=ANY($5))
            AND ($2='systems' OR ao.subject_id=s.environment_id::text)
            AND NOT EXISTS (SELECT 1 FROM user_attention_dismissals d
              WHERE d.occurrence_id=ao.id AND d.user_id=$1)
        ) SELECT (SELECT COUNT(*) FROM eligible),
          (SELECT array_agg(source_occurrence_key ORDER BY opened_at DESC,id DESC)
           FROM (SELECT * FROM eligible ORDER BY opened_at DESC,id DESC LIMIT 1000) page)"#,
    )
    .bind(user_id)
    .bind(category)
    .bind(observed_at)
    .bind(is_admin)
    .bind(environment_ids)
    .fetch_one(pool)
    .await?;
    Ok((total, keys.unwrap_or_default()))
}

/// Fetch all sidebar badge counts in one round-trip.
///
/// Attention counts come from eligible undismissed canonical occurrences in the
/// 24-hour window. Systems and environments are scoped to the requesting user's
/// environment memberships; admins see the fleet-wide counts.
///
/// All count and key queries enforce a bi-directional `opened_at` bound
/// (`cutoff <= opened_at <= observed_at`) so that occurrences created after
/// the cursor snapshot do not appear in the results, and the dismissal
/// endpoint cannot reject them for opening after the cursor.
/// Operational alerts require an underlying active managed machine. Registry
/// totals still include active external and reference registrations.
///
/// # Errors
/// Returns database errors when occurrence counts or keys cannot be read.
pub async fn fetch_navigation_badges(
    pool: &PgPool,
    user_id: Uuid,
    is_admin: bool,
    member_environment_ids: &[Uuid],
) -> Result<NavigationBadges> {
    let observed_at = capture_now(pool).await;

    let counts = attention::count_attention_for_user(
        pool,
        user_id,
        observed_at,
        is_admin,
        member_environment_ids,
    )
    .await?;

    // Fetch occurrence keys in parallel for the categories that need them.
    let (
        builds_occurrence_ids,
        evals_occurrence_ids,
        flakes_occurrence_ids,
        cves_occurrence_ids,
        systems_attention,
        environments_attention,
    ) = tokio::try_join!(
        attention::list_eligible_occurrence_keys(
            pool,
            user_id,
            "builds",
            observed_at,
            is_admin,
            None
        ),
        attention::list_eligible_occurrence_keys(
            pool,
            user_id,
            "evals",
            observed_at,
            is_admin,
            None
        ),
        attention::list_eligible_occurrence_keys(
            pool,
            user_id,
            "flakes",
            observed_at,
            is_admin,
            None
        ),
        attention::list_eligible_occurrence_keys(
            pool,
            user_id,
            "cves",
            observed_at,
            is_admin,
            None
        ),
        operational_attention(
            pool,
            user_id,
            "systems",
            observed_at,
            is_admin,
            member_environment_ids
        ),
        operational_attention(
            pool,
            user_id,
            "environments",
            observed_at,
            is_admin,
            member_environment_ids
        ),
    )?;

    Ok(NavigationBadges {
        observed_at: Some(observed_at),
        systems_attention: systems_attention.0,
        systems_total: total_systems(pool, is_admin, member_environment_ids)
            .await
            .unwrap_or(0),
        systems_fingerprint: None,
        flakes_errored: counts.flakes_errored,
        flakes_total: total_flakes(pool).await.unwrap_or(0),
        environments_attention: environments_attention.0,
        environments_total: total_environments(pool, is_admin, member_environment_ids)
            .await
            .unwrap_or(0),
        environments_fingerprint: None,
        builds_failed_new: counts.builds_failed_new,
        evals_failed_new: counts.evals_failed_new,
        cves_critical_new: counts.cves_critical_new,
        builds_occurrence_ids,
        evals_occurrence_ids,
        flakes_occurrence_ids,
        systems_occurrence_ids: systems_attention.1,
        environments_occurrence_ids: environments_attention.1,
        cves_occurrence_ids,
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

        let user_id = uuid::Uuid::new_v4();
        let badges = fetch_navigation_badges(&pool, user_id, true, &[])
            .await
            .expect("fetch_navigation_badges failed");

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

        let scoped = fetch_navigation_badges(&pool, user_id, false, &[member_env_id])
            .await
            .expect("scoped fetch_navigation_badges failed");
        let scoped_before = scoped.environments_total;

        let admin = fetch_navigation_badges(&pool, user_id, true, &[])
            .await
            .expect("admin fetch_navigation_badges failed");

        assert!(
            admin.environments_total >= scoped_before + 1,
            "admin total ({}) should include at least the extra unscoped environment beyond the member-scoped total ({})",
            admin.environments_total,
            scoped_before
        );

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
}
