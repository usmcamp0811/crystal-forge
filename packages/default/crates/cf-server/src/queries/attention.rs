//! Canonical attention-occurrence lifecycle and navigation badge queries.
//!
//! This module owns the single source of truth for "attention occurrences".
//! Each uninterrupted incident is represented by one immutable
//! `attention_occurrences` row. The row is attention-eligible for 24 hours
//! after `opened_at`, and it is removed from attention counts when a user
//! dismisses it via `user_attention_dismissals`.
//!
//! Categories and their stable source keys:
//!
//! * `builds`    -> `build:<job_id>` (terminal failure is immutable)
//! * `evals`     -> `eval:<commit_id>:<completed_at_microseconds>`
//! * `flakes`    -> `flake:<flake_id>:<episode_uuid>` (resolved/recovery opens a new episode)
//! * `systems`   -> `system:<system_id>:<reason>:<episode_uuid>`
//! * `environments` -> `environment:<environment_id>:<underlying_system_source_key>`
//! * `cves`      -> `cve:<cve_id>`
//!
//! The 24-hour eligibility rule is applied uniformly by all read paths; it is
//! a query predicate, not a cleanup requirement.

use anyhow::{Context, Result};
use chrono::{DateTime, Duration, Utc};
use sqlx::{PgPool, Postgres, Row};
use uuid::Uuid;

/// Attention window: occurrences are eligible for 24 hours from `opened_at`.
pub const ATTENTION_WINDOW: Duration = Duration::hours(24);

/// Returns true when an occurrence is within the attention window relative to
/// the supplied observation cursor.
///
/// The comparison is inclusive at the 24-hour boundary: an occurrence opened
/// exactly 24 hours ago is still attention-eligible.
pub fn is_attention_eligible(opened_at: DateTime<Utc>, observed_at: DateTime<Utc>) -> bool {
    opened_at >= observed_at - ATTENTION_WINDOW
}

/// Compute the cutoff timestamp for the attention window.
pub fn attention_cutoff(observed_at: DateTime<Utc>) -> DateTime<Utc> {
    observed_at - ATTENTION_WINDOW
}

/// Allow-listed attention categories. These must match the API/UI category
/// strings used by `NavigationBadges` and the dismissal endpoint.
const ALLOWED_CATEGORIES: &[&str] = &[
    "builds",
    "evals",
    "flakes",
    "systems",
    "environments",
    "cves",
];

fn validate_category(category: &str) -> Result<()> {
    if ALLOWED_CATEGORIES.contains(&category) {
        Ok(())
    } else {
        anyhow::bail!("invalid attention category: {category}")
    }
}

// ── Stable source occurrence keys ───────────────────────────────────────────

pub fn build_occurrence_key(job_id: Uuid) -> String {
    format!("build:{job_id}")
}

pub fn eval_occurrence_key(commit_id: i32, completed_at: DateTime<Utc>) -> String {
    let micros = completed_at.timestamp_micros().to_string();
    format!("eval:{commit_id}:{micros}")
}

pub fn flake_occurrence_key(flake_id: i32, episode_id: Uuid) -> String {
    format!("flake:{flake_id}:{episode_id}")
}

pub fn system_occurrence_key(system_id: Uuid, reason: &str, episode_id: Uuid) -> String {
    format!("system:{system_id}:{reason}:{episode_id}")
}

pub fn environment_occurrence_key(
    environment_id: Uuid,
    underlying_system_source_key: &str,
) -> String {
    format!("environment:{environment_id}:{underlying_system_source_key}")
}

pub fn cve_occurrence_key(cve_id: &str) -> String {
    format!("cve:{cve_id}")
}

// ── Core occurrence lifecycle ───────────────────────────────────────────────

/// Open a new occurrence, or observe an existing unresolved one with the same
/// deterministic source key. Deterministic keys are used for immutable
/// terminal events (`builds`, `evals`, `cves`) where the same key cannot recur.
///
/// For episode-based categories (`flakes`, `systems`, `environments`) use
/// [`open_or_observe_by_subject`] instead.
pub async fn open_or_observe<'e, E>(
    executor: E,
    category: &str,
    subject_type: &str,
    subject_id: &str,
    source_key: &str,
    opened_at: DateTime<Utc>,
    metadata: serde_json::Value,
) -> Result<Uuid>
where
    E: sqlx::Executor<'e, Database = Postgres>,
{
    let row = sqlx::query(
        r#"
        INSERT INTO attention_occurrences (
            category, subject_type, subject_id, source_occurrence_key,
            opened_at, last_observed_at, metadata
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        ON CONFLICT (category, source_occurrence_key) DO UPDATE
        SET last_observed_at = EXCLUDED.last_observed_at
        WHERE attention_occurrences.resolved_at IS NULL
        RETURNING id
        "#,
    )
    .bind(category)
    .bind(subject_type)
    .bind(subject_id)
    .bind(source_key)
    .bind(opened_at)
    .bind(opened_at)
    .bind(metadata)
    .fetch_one(executor)
    .await
    .context("failed to open/observe attention occurrence")?;

    Ok(row.get::<Uuid, _>("id"))
}

/// Open a new occurrence for an episode-based subject, or observe an existing
/// unresolved occurrence for the same subject and reason.
///
/// The `source_key_factory` is called with a fresh episode UUID when a new
/// occurrence is needed.
pub async fn open_or_observe_by_subject<F>(
    pool: &PgPool,
    category: &str,
    subject_type: &str,
    subject_id: &str,
    reason: &str,
    opened_at: DateTime<Utc>,
    metadata: serde_json::Value,
    source_key_factory: F,
) -> Result<Uuid>
where
    F: FnOnce(&str, Uuid) -> String,
{
    let mut tx = pool.begin().await.context("failed to begin occurrence transaction")?;

    let row = sqlx::query(
        r#"
        SELECT id
        FROM attention_occurrences
        WHERE category = $1
          AND subject_id = $2
          AND resolved_at IS NULL
          AND metadata @> $3::jsonb
        LIMIT 1
        FOR UPDATE
        "#,
    )
    .bind(category)
    .bind(subject_id)
    .bind(serde_json::json!({"reason": reason}))
    .fetch_optional(&mut *tx)
    .await
    .context("failed to find open attention occurrence")?;

    if let Some(id) = row.map(|r| r.get::<Uuid, _>("id")) {
        sqlx::query(
            "UPDATE attention_occurrences SET last_observed_at = $1 WHERE id = $2",
        )
        .bind(opened_at)
        .bind(id)
        .execute(&mut *tx)
        .await
        .context("failed to update last_observed_at")?;
        tx.commit().await.context("failed to commit occurrence update")?;
        return Ok(id);
    }

    let episode_id = Uuid::new_v4();
    let source_key = source_key_factory(reason, episode_id);
    let mut metadata = metadata;
    if let serde_json::Value::Object(ref mut map) = metadata {
        map.insert("reason".to_string(), serde_json::Value::String(reason.to_string()));
    }

    let row = sqlx::query(
        r#"
        INSERT INTO attention_occurrences (
            category, subject_type, subject_id, source_occurrence_key,
            opened_at, last_observed_at, metadata
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        RETURNING id
        "#,
    )
    .bind(category)
    .bind(subject_type)
    .bind(subject_id)
    .bind(source_key)
    .bind(opened_at)
    .bind(opened_at)
    .bind(metadata)
    .fetch_one(&mut *tx)
    .await
    .context("failed to open attention occurrence by subject")?;

    let id = row.get::<Uuid, _>("id");
    tx.commit().await.context("failed to commit occurrence insert")?;
    Ok(id)
}

/// Resolve a single open occurrence identified by its category, source kind,
/// and source id.
///
/// Returns the number of rows updated (0 or 1). Resolution is a one-way
/// transition: callers must open a new occurrence if the condition recurs.
pub async fn resolve(
    pool: &PgPool,
    category: &str,
    source_kind: &str,
    source_id: &str,
) -> Result<u64> {
    validate_category(category)?;

    let result = sqlx::query(
        r#"
        UPDATE attention_occurrences
        SET resolved_at = NOW()
        WHERE category = $1
          AND source_kind = $2
          AND source_id = $3
          AND resolved_at IS NULL
        "#,
    )
    .bind(category)
    .bind(source_kind)
    .bind(source_id)
    .execute(pool)
    .await
    .context("failed to resolve attention occurrence")?;

    Ok(result.rows_affected())
}

/// Resolve a single occurrence by id.
pub async fn resolve_occurrence<'e, E>(executor: E, occurrence_id: Uuid) -> Result<()>
where
    E: sqlx::Executor<'e, Database = Postgres>,
{
    sqlx::query(
        "UPDATE attention_occurrences SET resolved_at = NOW() WHERE id = $1 AND resolved_at IS NULL",
    )
    .bind(occurrence_id)
    .execute(executor)
    .await
    .context("failed to resolve attention occurrence")?;
    Ok(())
}

/// Resolve all open occurrences for a subject.
pub async fn resolve_open_occurrences_for_subject<'e, E>(
    executor: E,
    category: &str,
    subject_id: &str,
) -> Result<usize>
where
    E: sqlx::Executor<'e, Database = Postgres>,
{
    let result = sqlx::query(
        r#"
        UPDATE attention_occurrences
        SET resolved_at = NOW()
        WHERE category = $1
          AND subject_id = $2
          AND resolved_at IS NULL
        "#,
    )
    .bind(category)
    .bind(subject_id)
    .execute(executor)
    .await
    .context("failed to resolve open occurrences for subject")?;

    Ok(result.rows_affected() as usize)
}

/// Resolve all open occurrences for a subject, except those matching the given
/// reason. This is used when a system transitions from critical to offline:
/// the critical episode closes, a new offline episode opens.
pub async fn resolve_open_occurrences_except_reason<'e, E>(
    executor: E,
    category: &str,
    subject_id: &str,
    reason: &str,
) -> Result<usize>
where
    E: sqlx::Executor<'e, Database = Postgres>,
{
    let result = sqlx::query(
        r#"
        UPDATE attention_occurrences
        SET resolved_at = NOW()
        WHERE category = $1
          AND subject_id = $2
          AND resolved_at IS NULL
          AND NOT (metadata @> $3::jsonb)
        "#,
    )
    .bind(category)
    .bind(subject_id)
    .bind(serde_json::json!({"reason": reason}))
    .execute(executor)
    .await
    .context("failed to resolve open occurrences except reason")?;

    Ok(result.rows_affected() as usize)
}

// ── Dismissal ───────────────────────────────────────────────────────────────

/// Dismiss a bounded set of occurrences for a user. Each occurrence is validated:
/// * it must belong to the requested category
/// * it must have been opened at or before the observation cursor
///
/// Returns the updated per-category undismissed counts for the user.
pub async fn dismiss_occurrences(
    pool: &PgPool,
    user_id: Uuid,
    category: &str,
    observed_at: DateTime<Utc>,
    occurrence_ids: &[Uuid],
) -> Result<NavigationAttentionCounts> {
    let mut tx = pool.begin().await.context("failed to begin dismissal transaction")?;

    // Validate category and cursor for every requested occurrence.
    for id in occurrence_ids {
        let row = sqlx::query(
            "SELECT category, opened_at FROM attention_occurrences WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&mut *tx)
        .await
        .context("failed to look up occurrence for dismissal")?;

        let Some(row) = row else {
            anyhow::bail!("occurrence {} not found", id);
        };
        let occ_category: String = row.get("category");
        let opened_at: DateTime<Utc> = row.get("opened_at");
        if occ_category != category {
            anyhow::bail!("occurrence {} belongs to category {}, expected {}", id, occ_category, category);
        }
        if opened_at > observed_at {
            anyhow::bail!("occurrence {} opened after the observation cursor", id);
        }
    }

    // Idempotent insert of dismissals.
    if !occurrence_ids.is_empty() {
        let mut query = String::from(
            "INSERT INTO user_attention_dismissals (user_id, occurrence_id, dismissed_at) VALUES",
        );
        let now = Utc::now();
        for (idx, _id) in occurrence_ids.iter().enumerate() {
            if idx > 0 {
                query.push(',');
            }
            query.push_str(&format!(" (${}, ${}, ${})", idx * 3 + 1, idx * 3 + 2, idx * 3 + 3));
        }
        query.push_str(" ON CONFLICT (user_id, occurrence_id) DO NOTHING");

        let mut q = sqlx::query(&query);
        for id in occurrence_ids {
            q = q.bind(user_id);
            q = q.bind(id);
            q = q.bind(now);
        }
        q.execute(&mut *tx)
            .await
            .context("failed to insert dismissals")?;
    }

    tx.commit().await.context("failed to commit dismissal transaction")?;

    count_attention_for_user(pool, user_id, observed_at).await
}

// ── Navigation badge counts ─────────────────────────────────────────────────

/// Per-category attention counts returned by the badge endpoint.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NavigationAttentionCounts {
    pub systems_attention: i64,
    pub flakes_errored: i64,
    pub environments_attention: i64,
    pub builds_failed_new: i64,
    pub evals_failed_new: i64,
    pub cves_critical_new: i64,
}

/// Count eligible undismissed occurrences per category for the given user.
///
/// `is_admin` and `member_environment_ids` scope `systems` and `environments`
/// to the user's visible environments. Admins see the fleet-wide counts.
///
/// `observed_at` is returned as a cursor for the client to echo back on
/// dismissal.
pub async fn count_attention_for_user(
    pool: &PgPool,
    user_id: Uuid,
    observed_at: DateTime<Utc>,
) -> Result<NavigationAttentionCounts> {
    let cutoff = attention_cutoff(observed_at);

    let mut counts = NavigationAttentionCounts::default();

    // Systems and environments are scoped by environment membership.
    let scoped_ids: Vec<Uuid> = if user_id == Uuid::nil() {
        // Used by tests/admin paths; nil user means no scoping.
        Vec::new()
    } else {
        crate::queries::systems::get_user_environment_membership_ids(pool, user_id)
            .await
            .unwrap_or_default()
            .into_iter()
            .collect()
    };

    let counts_ref = &mut counts;
    count_category(
        pool,
        user_id,
        "builds",
        cutoff,
        None,
        &mut counts_ref.builds_failed_new,
    )
    .await?;
    count_category(
        pool,
        user_id,
        "evals",
        cutoff,
        None,
        &mut counts_ref.evals_failed_new,
    )
    .await?;
    count_category(
        pool,
        user_id,
        "flakes",
        cutoff,
        None,
        &mut counts_ref.flakes_errored,
    )
    .await?;
    count_category(
        pool,
        user_id,
        "cves",
        cutoff,
        None,
        &mut counts_ref.cves_critical_new,
    )
    .await?;
    count_category(
        pool,
        user_id,
        "systems",
        cutoff,
        Some(&scoped_ids),
        &mut counts_ref.systems_attention,
    )
    .await?;
    count_category(
        pool,
        user_id,
        "environments",
        cutoff,
        Some(&scoped_ids),
        &mut counts_ref.environments_attention,
    )
    .await?;

    Ok(counts)
}

async fn count_category(
    pool: &PgPool,
    user_id: Uuid,
    category: &str,
    cutoff: DateTime<Utc>,
    environment_ids: Option<&[Uuid]>,
    out: &mut i64,
) -> Result<()> {
    let sql = match category {
        "systems" => r#"
            SELECT COUNT(*)::bigint
            FROM attention_occurrences ao
            JOIN systems s ON s.id::text = ao.subject_id
            WHERE ao.category = 'systems'
              AND ao.resolved_at IS NULL
              AND ao.opened_at >= $1
              AND NOT EXISTS (
                  SELECT 1 FROM user_attention_dismissals uad
                  WHERE uad.occurrence_id = ao.id AND uad.user_id = $2
              )
              AND ($3 OR s.environment_id = ANY($4))
            "#,
        "environments" => r#"
            SELECT COUNT(*)::bigint
            FROM attention_occurrences ao
            JOIN environments e ON e.id::text = ao.subject_id
            WHERE ao.category = 'environments'
              AND ao.resolved_at IS NULL
              AND ao.opened_at >= $1
              AND NOT EXISTS (
                  SELECT 1 FROM user_attention_dismissals uad
                  WHERE uad.occurrence_id = ao.id AND uad.user_id = $2
              )
              AND ($3 OR e.id = ANY($4))
            "#,
        _ => r#"
            SELECT COUNT(*)::bigint
            FROM attention_occurrences ao
            WHERE ao.category = $5
              AND ao.resolved_at IS NULL
              AND ao.opened_at >= $1
              AND NOT EXISTS (
                  SELECT 1 FROM user_attention_dismissals uad
                  WHERE uad.occurrence_id = ao.id AND uad.user_id = $2
              )
            "#,
    };

    let row = if let Some(envs) = environment_ids {
        sqlx::query(sql)
            .bind(cutoff)
            .bind(user_id)
            .bind(user_id == Uuid::nil())
            .bind(envs)
            .bind(category)
            .fetch_one(pool)
            .await?
    } else {
        sqlx::query(sql)
            .bind(cutoff)
            .bind(user_id)
            .bind(false)
            .bind::<Vec<Uuid>>(vec![])
            .bind(category)
            .fetch_one(pool)
            .await?
    };

    *out = row.get::<i64, _>(0);
    Ok(())
}

// ── Occurrence retrieval (for acknowledgment payloads) ───────────────────────

/// Return the source occurrence keys for all eligible occurrences in a category
/// visible to the user. Used by the navigation endpoint to give the UI the exact
/// ids it can dismiss.
pub async fn list_eligible_occurrence_keys(
    pool: &PgPool,
    user_id: Uuid,
    category: &str,
    observed_at: DateTime<Utc>,
    environment_ids: Option<&[Uuid]>,
) -> Result<Vec<String>> {
    let cutoff = attention_cutoff(observed_at);
    let sql = match category {
        "systems" => r#"
            SELECT ao.source_occurrence_key
            FROM attention_occurrences ao
            JOIN systems s ON s.id::text = ao.subject_id
            WHERE ao.category = 'systems'
              AND ao.resolved_at IS NULL
              AND ao.opened_at >= $1
              AND NOT EXISTS (
                  SELECT 1 FROM user_attention_dismissals uad
                  WHERE uad.occurrence_id = ao.id AND uad.user_id = $2
              )
              AND ($3 OR s.environment_id = ANY($4))
            ORDER BY ao.opened_at DESC
            "#,
        "environments" => r#"
            SELECT ao.source_occurrence_key
            FROM attention_occurrences ao
            JOIN environments e ON e.id::text = ao.subject_id
            WHERE ao.category = 'environments'
              AND ao.resolved_at IS NULL
              AND ao.opened_at >= $1
              AND NOT EXISTS (
                  SELECT 1 FROM user_attention_dismissals uad
                  WHERE uad.occurrence_id = ao.id AND uad.user_id = $2
              )
              AND ($3 OR e.id = ANY($4))
            ORDER BY ao.opened_at DESC
            "#,
        _ => r#"
            SELECT ao.source_occurrence_key
            FROM attention_occurrences ao
            WHERE ao.category = $5
              AND ao.resolved_at IS NULL
              AND ao.opened_at >= $1
              AND NOT EXISTS (
                  SELECT 1 FROM user_attention_dismissals uad
                  WHERE uad.occurrence_id = ao.id AND uad.user_id = $2
              )
            ORDER BY ao.opened_at DESC
            LIMIT 10000
            "#,
    };

    let rows = if let Some(envs) = environment_ids {
        sqlx::query(sql)
            .bind(cutoff)
            .bind(user_id)
            .bind(user_id == Uuid::nil())
            .bind(envs)
            .bind(category)
            .fetch_all(pool)
            .await?
    } else {
        sqlx::query(sql)
            .bind(cutoff)
            .bind(user_id)
            .bind(false)
            .bind::<Vec<Uuid>>(vec![])
            .bind(category)
            .fetch_all(pool)
            .await?
    };

    Ok(rows.into_iter().map(|r| r.get::<String, _>(0)).collect())
}

/// Translate a list of source occurrence keys back into occurrence IDs.
pub async fn occurrence_ids_by_keys(
    pool: &PgPool,
    category: &str,
    keys: &[String],
) -> Result<Vec<Uuid>> {
    if keys.is_empty() {
        return Ok(Vec::new());
    }

    let query = String::from(
        "SELECT id FROM attention_occurrences WHERE category = $1 AND source_occurrence_key = ANY($2)",
    );
    let rows = sqlx::query(&query)
        .bind(category)
        .bind(keys)
        .fetch_all(pool)
        .await
        .context("failed to resolve occurrence keys")?;

    Ok(rows.into_iter().map(|r| r.get::<Uuid, _>("id")).collect())
}

// ── Retention cleanup ───────────────────────────────────────────────────────

/// Run bounded cleanup of resolved occurrences and orphaned dismissals.
/// Returns the number of deleted occurrence and dismissal rows.
pub async fn cleanup(
    pool: &PgPool,
    resolved_retention: Duration,
    batch_size: i64,
) -> Result<(i64, i64)> {
    let row = sqlx::query(
        "SELECT cleanup_attention_occurrences($1, $2) AS result",
    )
    .bind(resolved_retention)
    .bind(batch_size)
    .fetch_one(pool)
    .await
    .context("cleanup_attention_occurrences failed")?;

    let result: (i64, i64) = row.get("result");
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn attention_eligibility_inclusive_at_boundary() {
        let observed = Utc::now();
        let cutoff = observed - ATTENTION_WINDOW;
        assert!(is_attention_eligible(cutoff, observed));
        assert!(!is_attention_eligible(cutoff - Duration::microseconds(1), observed));
    }

    #[test]
    fn build_key_is_stable() {
        let job_id = Uuid::new_v4();
        assert_eq!(build_occurrence_key(job_id), format!("build:{job_id}"));
    }

    #[test]
    fn eval_key_includes_microseconds() {
        let ts = DateTime::from_timestamp(1_000_000, 123_456_789).unwrap();
        let key = eval_occurrence_key(42, ts);
        assert!(key.starts_with("eval:42:"));
        let suffix = key.split(':').last().unwrap();
        assert_eq!(suffix.parse::<i64>().unwrap(), ts.timestamp_micros());
    }

    #[test]
    fn system_key_includes_reason_and_episode() {
        let system_id = Uuid::new_v4();
        let episode_id = Uuid::new_v4();
        let key = system_occurrence_key(system_id, "offline", episode_id);
        assert_eq!(key, format!("system:{system_id}:offline:{episode_id}"));
    }
}
