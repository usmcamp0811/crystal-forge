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
//! * `cves`      -> `cve:<cve_id>:<episode_uuid>` (fleet-relevance episode; resolving and
//!   later recurring as fleet-relevant opens a new episode, mirroring flakes/systems)
//!
//! The 24-hour eligibility rule is applied uniformly by all read paths; it is
//! a query predicate, not a cleanup requirement.

use anyhow::{Context, Result};
use chrono::{DateTime, Duration, Utc};
use sqlx::{PgPool, Postgres, Row};
use tracing::warn;
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

pub fn environment_occurrence_key(environment_id: Uuid, episode_id: Uuid) -> String {
    format!("environment:{environment_id}:{episode_id}")
}

/// Build the source occurrence key for a CVE fleet-relevance episode. A CVE
/// can leave and later re-enter fleet relevance (patched everywhere, then
/// reintroduced, or rescored back to critical); each such episode gets a
/// fresh `episode_id` via [`open_or_observe_by_subject`] rather than reusing
/// a single deterministic key, so recurrence after resolution is representable.
pub fn cve_occurrence_key(cve_id: &str, episode_id: Uuid) -> String {
    format!("cve:{cve_id}:{episode_id}")
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
        SET last_observed_at = GREATEST(attention_occurrences.last_observed_at, EXCLUDED.last_observed_at)
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
///
/// Concurrent producer/reconciler runs for the same `(category, subject_id,
/// reason)` are serialized with a transaction-scoped PostgreSQL advisory
/// lock. Without it, two concurrent callers could both observe "no open row"
/// (there is nothing yet to lock with `SELECT ... FOR UPDATE`) and each
/// insert a distinct, randomly keyed episode — the unique constraint on
/// `(category, source_occurrence_key)` would not catch this because the two
/// generated keys differ. The advisory lock is released automatically at
/// commit/rollback, so it never outlives this call.
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
    let mut tx = pool
        .begin()
        .await
        .context("failed to begin occurrence transaction")?;

    let lock_key = format!("attention_occurrence:{category}:{subject_id}:{reason}");
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(&lock_key)
        .execute(&mut *tx)
        .await
        .context("failed to acquire occurrence lock")?;

    // Inject `reason` into metadata BEFORE the lookup so the
    // UPDATE path preserves it. Without this, the UPDATE on line 220
    // would overwrite metadata with the caller's value (which may not
    // include `reason`), causing a third call to NOT find the row and
    // insert a duplicate.
    let mut metadata = metadata;
    if let serde_json::Value::Object(ref mut map) = metadata {
        map.insert(
            "reason".to_string(),
            serde_json::Value::String(reason.to_string()),
        );
    }

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
        // Update both `last_observed_at` AND `metadata` so re-observing
        // after an earlier transient failure reflects the latest diagnostic
        // details (e.g. the most recent sync error message), not the first
        // one that opened the episode. `metadata` now includes `reason`
        // because we injected it above, so the lookup on the next call
        // will still succeed.
        //
        // Condition metadata replacement on observation ordering so an
        // older caller that acquires the lock later cannot overwrite
        // newer diagnostic information with stale metadata.
        sqlx::query(
            "UPDATE attention_occurrences \
             SET metadata = CASE WHEN $1 >= last_observed_at THEN $2 ELSE metadata END, \
                 last_observed_at = GREATEST(last_observed_at, $1) \
             WHERE id = $3",
        )
        .bind(opened_at)
        .bind(&metadata)
        .bind(id)
        .execute(&mut *tx)
        .await
        .context("failed to update last_observed_at and metadata")?;
        tx.commit()
            .await
            .context("failed to commit occurrence update")?;
        return Ok(id);
    }

    let episode_id = Uuid::new_v4();
    let source_key = source_key_factory(reason, episode_id);

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
    tx.commit()
        .await
        .context("failed to commit occurrence insert")?;
    Ok(id)
}

/// Atomically transition a subject to a new reason: if the current open
/// occurrence already has the given `reason`, observe it (update
/// `last_observed_at` and `metadata`); otherwise, resolve *all* open
/// occurrences and open a new one for the current `reason`.
///
/// Unlike [`open_or_observe_by_subject`], this uses a reason-independent
/// lock (`attention_occurrence:{category}:{subject_id}`) so that a
/// concurrent call to transition the same subject from any reason to any
/// other reason is serialized. This prevents two concurrent transitions
/// from racing (e.g. reconciler opens `stale_sync` while the sync itself
/// fails and opens `sync_error` — without a shared lock they would create
/// two open occurrences for what is conceptually one incident).
///
/// When the reason hasn't changed, the existing occurrence's `id`,
/// `source_occurrence_key`, and `opened_at` are preserved so the episode
/// represents one uninterrupted incident. This is essential for periodic
/// reconcilers that call transition repeatedly on the same condition —
/// without it, every sweep would generate a new episode with a new key,
/// resetting `opened_at` and invalidating any user dismissal for the
/// previous key.
pub async fn transition_by_subject<F>(
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
    let mut tx = pool
        .begin()
        .await
        .context("failed to begin transition transaction")?;

    // Reason-independent lock so stale_sync ↔ sync_error transitions are
    // serialized.
    let lock_key = format!("attention_occurrence:{category}:{subject_id}");
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(&lock_key)
        .execute(&mut *tx)
        .await
        .context("failed to acquire transition lock")?;

    // Inject `reason` into metadata early so both paths preserve it.
    let mut metadata = metadata;
    if let serde_json::Value::Object(ref mut map) = metadata {
        map.insert(
            "reason".to_string(),
            serde_json::Value::String(reason.to_string()),
        );
    }

    // Check if there is already an open occurrence with the same reason.
    let existing = sqlx::query(
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
    .context("failed to find existing occurrence in transition")?;

    if let Some(row) = existing {
        let id: Uuid = row.get("id");
        // Reason matches — just observe the existing occurrence.
        // Condition metadata replacement on observation ordering so an
        // older caller that acquires the lock later cannot overwrite
        // newer diagnostic information with stale metadata.
        sqlx::query(
            "UPDATE attention_occurrences \
             SET metadata = CASE WHEN $1 >= last_observed_at THEN $2 ELSE metadata END, \
                 last_observed_at = GREATEST(last_observed_at, $1) \
             WHERE id = $3",
        )
        .bind(opened_at)
        .bind(&metadata)
        .bind(id)
        .execute(&mut *tx)
        .await
        .context("failed to update occurrence in transition")?;
        tx.commit()
            .await
            .context("failed to commit transition observe")?;
        return Ok(id);
    }

    // Reason differs or no occurrence exists — resolve all open occurrences
    // and insert a new one.
    sqlx::query(
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
    .execute(&mut *tx)
    .await
    .context("failed to resolve open occurrences in transition")?;

    let episode_id = Uuid::new_v4();
    let source_key = source_key_factory(reason, episode_id);

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
    .context("failed to insert transition occurrence")?;

    let id = row.get::<Uuid, _>("id");
    tx.commit()
        .await
        .context("failed to commit transition")?;
    Ok(id)
}

/// Authoritative per-CVE attention reconciliation.
///
/// This is the single CVE attention lifecycle helper.  Every path that
/// changes a CVE's attention state (scan save, periodic reconciliation, stale
/// resolution, startup duplicate repair) must go through here.  It:
///
/// 1. Acquires the per-CVE advisory lock (matching the lock key used
///    historically by the scan-save producer).
/// 2. Rechecks fleet relevance against the current database view.
/// 3. Maintains `cves.fleet_relevant_since` so the episode start timestamp is
///    persisted when a CVE transitions to relevant and cleared when it leaves.
/// 4. Opens exactly one open occurrence when relevant, observing an existing
///    one, and resolves all open occurrences when not relevant.
///
/// The episode `opened_at` is taken from the persisted `fleet_relevant_since`
/// if available, then from an existing open occurrence's `opened_at` (used to
/// backfill pre-migration rows), and finally falls back to now() for a brand
/// new episode.  This prevents a genuine recurrence from being backdated to an
/// old scan (round 16 review).
///
/// This wrapper starts a new transaction.  Callers that are already inside a
/// transaction should use `reconcile_cve_attention_subject_tx`.
pub async fn reconcile_cve_attention_subject(pool: &PgPool, cve_id: &str) -> Result<()> {
    let mut tx = pool.begin().await.context("begin CVE attention tx")?;
    reconcile_cve_attention_subject_tx(&mut tx, cve_id).await?;
    tx.commit().await.context("commit CVE attention tx")?;
    Ok(())
}

/// In-transaction version of [`reconcile_cve_attention_subject`].
///
/// Runs inside the supplied transaction, so it is suitable for callers that
/// already manage a transaction (e.g., startup duplicate repair).  Does NOT
/// commit or roll back the transaction.
pub(crate) async fn reconcile_cve_attention_subject_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    cve_id: &str,
) -> Result<()> {
    let lock_key = format!("attention_occurrence:cves:{cve_id}:critical");
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(&lock_key)
        .execute(&mut **tx)
        .await
        .context("acquire CVE attention lock")?;

    // Recheck fleet relevance under the lock.  A CVE that was selected as
    // stale may have become relevant again by the time we acquire the lock;
    // without this recheck we could resolve a newly valid occurrence.
    let relevant: bool = sqlx::query_scalar(
        r#"
        SELECT EXISTS (
            SELECT 1
            FROM view_cve_list_with_metadata
            WHERE cve_id = $1
              AND severity = 'CRITICAL'
              AND affected_count > 0
        )
        "#,
    )
    .bind(cve_id)
    .fetch_one(&mut **tx)
    .await
    .context("recheck CVE fleet relevance")?;

    // Read (and lock) the persisted episode-start timestamp.
    let persisted_since: Option<DateTime<Utc>> = sqlx::query_scalar(
        "SELECT fleet_relevant_since FROM cves WHERE id = $1 FOR UPDATE",
    )
    .bind(cve_id)
    .fetch_one(&mut **tx)
    .await
    .context("read cves.fleet_relevant_since")?;

    if relevant {
        let now = Utc::now();

        // If an open occurrence already exists (e.g. from before migration
        // 0182), use its opened_at to backfill the missing episode timestamp.
        let existing_opened_at: Option<DateTime<Utc>> = sqlx::query_scalar(
            r#"
            SELECT MIN(opened_at)
            FROM attention_occurrences
            WHERE category = 'cves'
              AND subject_id = $1
              AND resolved_at IS NULL
            "#,
        )
        .bind(cve_id)
        .fetch_one(&mut **tx)
        .await
        .context("read existing CVE opened_at")?;

        let episode_started_at = persisted_since
            .or(existing_opened_at)
            .unwrap_or(now);

        // Persist the episode start timestamp if it is missing or changed.
        if persisted_since != Some(episode_started_at) {
            sqlx::query(
                "UPDATE cves SET fleet_relevant_since = $1, updated_at = NOW() WHERE id = $2",
            )
            .bind(episode_started_at)
            .bind(cve_id)
            .execute(&mut **tx)
            .await
            .context("persist cves.fleet_relevant_since")?;
        }

        // Open a new occurrence or observe an existing one.
        let existing: Option<Uuid> = sqlx::query_scalar(
            r#"
            SELECT id
            FROM attention_occurrences
            WHERE category = 'cves'
              AND subject_id = $1
              AND resolved_at IS NULL
            LIMIT 1
            FOR UPDATE
            "#,
        )
        .bind(cve_id)
        .fetch_optional(&mut **tx)
        .await
        .context("find existing CVE occurrence")?;

        let metadata = serde_json::json!({
            "reason": "critical",
            "cve_id": cve_id,
        });

        if let Some(id) = existing {
            sqlx::query(
                "UPDATE attention_occurrences \
                 SET last_observed_at = GREATEST(last_observed_at, $1), \
                     metadata = CASE WHEN $1 >= last_observed_at THEN $2 ELSE metadata END \
                 WHERE id = $3",
            )
            .bind(now)
            .bind(&metadata)
            .bind(id)
            .execute(&mut **tx)
            .await
            .context("observe CVE occurrence")?;
        } else {
            let episode_id = Uuid::new_v4();
            let source_key = cve_occurrence_key(cve_id, episode_id);
            sqlx::query(
                r#"
                INSERT INTO attention_occurrences (
                    category, subject_type, subject_id, source_occurrence_key,
                    opened_at, last_observed_at, metadata
                )
                VALUES ('cves', 'cve', $1, $2, $3, $4, $5)
                "#,
            )
            .bind(cve_id)
            .bind(source_key)
            .bind(episode_started_at)
            .bind(now)
            .bind(&metadata)
            .execute(&mut **tx)
            .await
            .context("open CVE occurrence")?;
        }
    } else {
        // Resolve every open occurrence and clear the episode start so a
        // future recurrence begins a fresh episode.
        sqlx::query(
            "UPDATE attention_occurrences \
             SET resolved_at = statement_timestamp() \
             WHERE category = 'cves' AND subject_id = $1 AND resolved_at IS NULL",
        )
        .bind(cve_id)
        .execute(&mut **tx)
        .await
        .context("resolve CVE occurrences")?;

        if persisted_since.is_some() {
            sqlx::query(
                "UPDATE cves SET fleet_relevant_since = NULL, updated_at = NOW() WHERE id = $1",
            )
            .bind(cve_id)
            .execute(&mut **tx)
            .await
            .context("clear cves.fleet_relevant_since")?;
        }
    }

    Ok(())
}

/// Resolve a single open occurrence identified by its category, subject type,
/// and subject id.
///
/// Returns the number of rows updated (0 or 1). Resolution is a one-way
/// transition: callers must open a new occurrence if the condition recurs.
///
/// `subject_type` and `subject_id` must match the columns of the same name on
/// `attention_occurrences` (the schema has no separate `source_kind`/
/// `source_id` columns).
pub async fn resolve(
    pool: &PgPool,
    category: &str,
    subject_type: &str,
    subject_id: &str,
) -> Result<u64> {
    validate_category(category)?;

    let result = sqlx::query(
        r#"
        UPDATE attention_occurrences
        SET resolved_at = NOW()
        WHERE category = $1
          AND subject_type = $2
          AND subject_id = $3
          AND resolved_at IS NULL
        "#,
    )
    .bind(category)
    .bind(subject_type)
    .bind(subject_id)
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

/// Resolve all open occurrences for a subject, serialized with the same
/// subject-level advisory lock used by [`transition_by_subject`] and the
/// stale reconciler. This prevents a race where a concurrent transition
/// (e.g. reconciler inserting `stale_sync`) runs immediately after the
/// resolve and creates a new open occurrence for a subject that should no
/// longer have one.
///
/// Acquires the reason-independent lock
/// `attention_occurrence:{category}:{subject_id}` (matching
/// [`transition_by_subject`]), then resolves all open occurrences for the
/// subject, then commits.
pub async fn resolve_under_lock(
    pool: &PgPool,
    category: &str,
    subject_id: &str,
) -> Result<usize> {
    validate_category(category)?;

    let mut tx = pool
        .begin()
        .await
        .context("failed to begin resolve-under-lock transaction")?;

    let lock_key = format!("attention_occurrence:{category}:{subject_id}");
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(&lock_key)
        .execute(&mut *tx)
        .await
        .context("failed to acquire resolve-under-lock advisory lock")?;

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
    .execute(&mut *tx)
    .await
    .context("failed to resolve open occurrences under lock")?;

    tx.commit()
        .await
        .context("failed to commit resolve-under-lock transaction")?;

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

/// Resolve every open environment occurrence tied to a specific underlying
/// system, REGARDLESS of which environment (`subject_id`) it belongs to.
///
/// Used when a system recovers to healthy/warning: a system that was
/// critical in environment A and has since moved to environment B (or been
/// unassigned) must not leave A's derived occurrence open just because the
/// resolution was scoped to the system's *current* environment. Scoped
/// purely by `underlying_system_id` metadata — deliberately not filtered by
/// `environment_id` — and resolves every matching row rather than an
/// arbitrary single one, so a prior bug or race that left more than one
/// open occurrence for the same system cannot leave a duplicate
/// permanently unresolved.
pub async fn resolve_environment_occurrences_for_system_any_environment<'e, E>(
    executor: E,
    system_id: Uuid,
) -> Result<usize>
where
    E: sqlx::Executor<'e, Database = Postgres>,
{
    let result = sqlx::query(
        r#"
        UPDATE attention_occurrences
        SET resolved_at = NOW()
        WHERE category = 'environments'
          AND metadata @> $1::jsonb
          AND resolved_at IS NULL
        "#,
    )
    .bind(serde_json::json!({"underlying_system_id": system_id.to_string()}))
    .execute(executor)
    .await
    .context("failed to resolve environment occurrences for system across all environments")?;

    Ok(result.rows_affected() as usize)
}

// ── Dismissal ───────────────────────────────────────────────────────────────

/// Dismiss a bounded set of occurrences for a user. Each occurrence is supplied
/// as a canonical source occurrence key and is validated:
/// * it must exist and be visible to the requesting user
/// * it must belong to the requested category
/// * it must have been opened at or before the observation cursor
///
/// Visibility mirrors the same environment-membership scoping used by the
/// badge/list queries: `systems` and `environments` occurrences are
/// restricted to environments in `member_environment_ids` unless `is_admin`;
/// other categories are not environment-scoped. A caller must not be able to
/// dismiss, or distinguish the existence of, an occurrence outside their
/// visibility from one that genuinely does not exist — both fail with the
/// same message.
///
/// Validation is performed as a single set-based query with `ANY($keys)`
/// rather than one query per supplied key, avoiding up to 1,000 sequential
/// database round trips for a full badge batch.
///
/// Returns the updated per-category undismissed counts for the user.
pub async fn dismiss_occurrences(
    pool: &PgPool,
    user_id: Uuid,
    category: &str,
    observed_at: DateTime<Utc>,
    occurrence_keys: &[String],
    is_admin: bool,
    member_environment_ids: &[Uuid],
) -> Result<NavigationAttentionCounts> {
    validate_category(category)?;

    if occurrence_keys.is_empty() {
        return count_attention_for_user(pool, user_id, observed_at, is_admin, member_environment_ids)
            .await;
    }

    let mut tx = pool
        .begin()
        .await
        .context("failed to begin dismissal transaction")?;

    // Validate all keys in a single query. Returns the rows that exist,
    // belong to the requested category, and are within the cursor bound.
    // For non-admin callers, also filter by environment membership.
    let rows: Vec<(String, chrono::DateTime<Utc>, String, Option<Uuid>)> = sqlx::query_as(
        r#"
        SELECT ao.source_occurrence_key, ao.opened_at, ao.subject_id,
               s.environment_id AS system_environment_id
        FROM attention_occurrences ao
        LEFT JOIN systems s
          ON ao.category = 'systems' AND s.id::text = ao.subject_id
        WHERE ao.source_occurrence_key = ANY($1)
          AND ao.category = $2
          AND ao.opened_at <= $3
        "#,
    )
    .bind(occurrence_keys)
    .bind(category)
    .bind(observed_at)
    .fetch_all(&mut *tx)
    .await
    .context("failed to validate occurrence keys for dismissal")?;

    // Verify every requested key is present in the result set. If a key is
    // missing, it either doesn't exist, belongs to a different category, or
    // opened after the cursor — indistinguishable to the caller.
    let validated_keys: std::collections::HashSet<String> =
        rows.iter().map(|(k, _, _, _)| k.clone()).collect();
    for key in occurrence_keys {
        if !validated_keys.contains(key) {
            anyhow::bail!("occurrence key '{key}' is not available for dismissal");
        }
    }

    // For non-admin users, apply environment-scoped visibility.
    if !is_admin {
        for (key, _opened_at, subject_id, system_environment_id) in &rows {
            let visible = match category {
                "systems" => system_environment_id
                    .is_some_and(|env_id| member_environment_ids.contains(&env_id)),
                "environments" => subject_id
                    .parse::<Uuid>()
                    .is_ok_and(|env_id| member_environment_ids.contains(&env_id)),
                _ => true,
            };
            if !visible {
                anyhow::bail!("occurrence key '{key}' is not available for dismissal");
            }
        }
    }

    // Insert dismissals for all validated occurrence keys in one statement
    // using INSERT ... SELECT, avoiding individual lookups.
    sqlx::query(
        r#"
        INSERT INTO user_attention_dismissals (user_id, occurrence_id, dismissed_at)
        SELECT $1, ao.id, NOW()
        FROM attention_occurrences ao
        WHERE ao.source_occurrence_key = ANY($2)
          AND ao.category = $3
        ON CONFLICT (user_id, occurrence_id) DO NOTHING
        "#,
    )
    .bind(user_id)
    .bind(occurrence_keys)
    .bind(category)
    .execute(&mut *tx)
    .await
    .context("failed to insert dismissals")?;

    tx.commit()
        .await
        .context("failed to commit dismissal transaction")?;

    count_attention_for_user(pool, user_id, observed_at, is_admin, member_environment_ids).await
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
    is_admin: bool,
    member_environment_ids: &[Uuid],
) -> Result<NavigationAttentionCounts> {
    let cutoff = attention_cutoff(observed_at);

    let mut counts = NavigationAttentionCounts::default();

    // Systems and environments are scoped by environment membership.
    let scoped_ids: Vec<Uuid> = if is_admin {
        Vec::new()
    } else {
        member_environment_ids.to_vec()
    };

    let counts_ref = &mut counts;
    count_category(
        pool,
        user_id,
        "builds",
        cutoff,
        observed_at,
        None,
        is_admin,
        &mut counts_ref.builds_failed_new,
    )
    .await?;
    count_category(
        pool,
        user_id,
        "evals",
        cutoff,
        observed_at,
        None,
        is_admin,
        &mut counts_ref.evals_failed_new,
    )
    .await?;
    count_category(
        pool,
        user_id,
        "flakes",
        cutoff,
        observed_at,
        None,
        is_admin,
        &mut counts_ref.flakes_errored,
    )
    .await?;
    count_category(
        pool,
        user_id,
        "cves",
        cutoff,
        observed_at,
        None,
        is_admin,
        &mut counts_ref.cves_critical_new,
    )
    .await?;
    count_category(
        pool,
        user_id,
        "systems",
        cutoff,
        observed_at,
        Some(&scoped_ids),
        is_admin,
        &mut counts_ref.systems_attention,
    )
    .await?;
    count_category(
        pool,
        user_id,
        "environments",
        cutoff,
        observed_at,
        Some(&scoped_ids),
        is_admin,
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
    observed_at: DateTime<Utc>,
    environment_ids: Option<&[Uuid]>,
    is_admin: bool,
    out: &mut i64,
) -> Result<()> {
    let sql = match category {
        "systems" => {
            r#"
            SELECT COUNT(*)::bigint
            FROM attention_occurrences ao
            JOIN systems s ON s.id::text = ao.subject_id
            WHERE ao.category = 'systems'
              AND ao.resolved_at IS NULL
              AND ao.opened_at >= $1
              AND ao.opened_at <= $6
              AND NOT EXISTS (
                  SELECT 1 FROM user_attention_dismissals uad
                  WHERE uad.occurrence_id = ao.id AND uad.user_id = $2
              )
              AND ($3 OR s.environment_id = ANY($4))
            "#
        }
        "environments" => {
            r#"
            SELECT COUNT(*)::bigint
            FROM attention_occurrences ao
            JOIN environments e ON e.id::text = ao.subject_id
            WHERE ao.category = 'environments'
              AND ao.resolved_at IS NULL
              AND ao.opened_at >= $1
              AND ao.opened_at <= $6
              AND NOT EXISTS (
                  SELECT 1 FROM user_attention_dismissals uad
                  WHERE uad.occurrence_id = ao.id AND uad.user_id = $2
              )
              AND ($3 OR e.id = ANY($4))
            "#
        }
        _ => {
            r#"
            SELECT COUNT(*)::bigint
            FROM attention_occurrences ao
            WHERE ao.category = $5
              AND ao.resolved_at IS NULL
              AND ao.opened_at >= $1
              AND ao.opened_at <= $6
              AND NOT EXISTS (
                  SELECT 1 FROM user_attention_dismissals uad
                  WHERE uad.occurrence_id = ao.id AND uad.user_id = $2
              )
            "#
        }
    };

    let row = if let Some(envs) = environment_ids {
        sqlx::query(sql)
            .bind(cutoff)
            .bind(user_id)
            .bind(is_admin)
            .bind(envs)
            .bind(category)
            .bind(observed_at)
            .fetch_one(pool)
            .await?
    } else {
        sqlx::query(sql)
            .bind(cutoff)
            .bind(user_id)
            .bind(is_admin)
            .bind::<Vec<Uuid>>(vec![])
            .bind(category)
            .bind(observed_at)
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
    is_admin: bool,
    environment_ids: Option<&[Uuid]>,
) -> Result<Vec<String>> {
    let cutoff = attention_cutoff(observed_at);
    let sql = match category {
        "systems" => {
            r#"
            SELECT ao.source_occurrence_key
            FROM attention_occurrences ao
            JOIN systems s ON s.id::text = ao.subject_id
            WHERE ao.category = 'systems'
              AND ao.resolved_at IS NULL
              AND ao.opened_at >= $1
              AND ao.opened_at <= $6
              AND NOT EXISTS (
                  SELECT 1 FROM user_attention_dismissals uad
                  WHERE uad.occurrence_id = ao.id AND uad.user_id = $2
              )
              AND ($3 OR s.environment_id = ANY($4))
            ORDER BY ao.opened_at DESC
            LIMIT 1000
            "#
        }
        "environments" => {
            r#"
            SELECT ao.source_occurrence_key
            FROM attention_occurrences ao
            JOIN environments e ON e.id::text = ao.subject_id
            WHERE ao.category = 'environments'
              AND ao.resolved_at IS NULL
              AND ao.opened_at >= $1
              AND ao.opened_at <= $6
              AND NOT EXISTS (
                  SELECT 1 FROM user_attention_dismissals uad
                  WHERE uad.occurrence_id = ao.id AND uad.user_id = $2
              )
              AND ($3 OR e.id = ANY($4))
            ORDER BY ao.opened_at DESC
            LIMIT 1000
            "#
        }
        _ => {
            r#"
            SELECT ao.source_occurrence_key
            FROM attention_occurrences ao
            WHERE ao.category = $5
              AND ao.resolved_at IS NULL
              AND ao.opened_at >= $1
              AND ao.opened_at <= $6
              AND NOT EXISTS (
                  SELECT 1 FROM user_attention_dismissals uad
                  WHERE uad.occurrence_id = ao.id AND uad.user_id = $2
              )
            ORDER BY ao.opened_at DESC
            LIMIT 1000
            "#
        }
    };

    let rows = if let Some(envs) = environment_ids {
        sqlx::query(sql)
            .bind(cutoff)
            .bind(user_id)
            .bind(is_admin)
            .bind(envs)
            .bind(category)
            .bind(observed_at)
            .fetch_all(pool)
            .await?
    } else {
        sqlx::query(sql)
            .bind(cutoff)
            .bind(user_id)
            .bind(is_admin)
            .bind::<Vec<Uuid>>(vec![])
            .bind(category)
            .bind(observed_at)
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
    batch_size: i32,
) -> Result<(i64, i64)> {
    // Two independent runtime type-mismatch bugs existed here, neither
    // caught by `cargo check` because this is an unprepared runtime
    // `sqlx::query` (not `sqlx::query!`):
    //   1. `chrono::Duration` has no `Encode<Postgres>` for the `interval`
    //      wire type — convert to `PgInterval` explicitly.
    //   2. `cleanup_attention_occurrences`'s second parameter is `INT`
    //      (4 bytes) in the migration, but this previously bound an `i64`
    //      (`bigint`); Postgres does not implicitly resolve that overload,
    //      so every call failed with "function ... does not exist".
    let interval = sqlx::postgres::types::PgInterval {
        months: 0,
        days: 0,
        microseconds: resolved_retention.num_microseconds().unwrap_or(i64::MAX),
    };

    // `cleanup_attention_occurrences` is a RETURNS TABLE function.
    // `SELECT cleanup_attention_occurrences(...) AS result` returns one
    // column holding an anonymous composite row value (Postgres renders it
    // as e.g. `(1,0)`), which sqlx cannot decode as `(i64, i64)` without a
    // matching composite `Decode` impl. Calling it in the FROM clause
    // instead correctly projects it into two typed columns.
    let row = sqlx::query("SELECT * FROM cleanup_attention_occurrences($1, $2)")
        .bind(interval)
        .bind(batch_size)
        .fetch_one(pool)
        .await
        .context("cleanup_attention_occurrences failed")?;

    let deleted_occurrences: i64 = row.get("deleted_occurrences");
    let deleted_dismissals: i64 = row.get("deleted_dismissals");
    Ok((deleted_occurrences, deleted_dismissals))
}

/// Idempotent repair for duplicate OPEN occurrences that may have
/// accumulated from an earlier reconciliation-logic bug (multiple attention
/// producer/reconciler races have been found and fixed across MR !307's
/// review history; this repairs any leftover duplicates regardless of which
/// one caused them).
///
/// The base invariant is: at most one open (`resolved_at IS NULL`)
/// occurrence per `(category, subject_id)`. Repair strategy is split into
/// three families, chosen per category:
///
/// **Merge-with-dismissal-migration** (only `evals` and `builds`): safe
/// *only* when two rows sharing an identity are PROVABLY the same
/// real-world event, so transferring a dismissal from one to the other
/// cannot hide or falsely-dismiss an unrelated incident.
///
/// * `evals` — grouping additionally requires an exact `opened_at` match.
///   `opened_at` is the evaluation's own `evaluation_completed_at`, an
///   external timestamp that can only coincide for two rows if they are
///   the same real completion event — regardless of *which* historical bug
///   produced two rows with matching `opened_at` but different
///   `source_occurrence_key`s (an earlier hypothesis attributing this to
///   `EXTRACT(EPOCH ...)` float imprecision was reviewed and rejected —
///   PostgreSQL 16's `EXTRACT` returns exact `numeric`, not `float8` — and
///   the true cause of any specific observed inflation, e.g. a previously
///   reported count of duplicate eval rows on a deployed instance, has not
///   been independently confirmed against that instance's actual data; see
///   the diagnostic query below). A row for a genuinely different, older
///   failure episode has a different `opened_at` and is never grouped with
///   a newer one here — the eval reconciliation safety net
///   (`reconcile_terminal_events`'s stale-occurrence pass) resolves it
///   independently once it no longer matches the commit's current state.
/// * `builds` — the occurrence key has no episode/timestamp component at
///   all (`build:<job_id>`, and a re-queued job always gets a brand-new
///   `job_id`), so a build job can never legitimately recur under the same
///   subject id; two open rows for one `subject_id` can only be an
///   artifact of a key-encoding bug, i.e. provably the same event.
///
/// **Resolve all, reconstruct from authoritative state** (everything else,
/// including `systems`, `flakes`, `cves`, `environments`, and any category
/// not explicitly listed above): these are episode-based —
/// `transition_by_subject` mints a fresh episode UUID on every reason
/// change, `environments` mints one on every derived transition — so two
/// open rows sharing an identity are NOT provably the same episode; they
/// may be two genuinely distinct incidents left open by an unrelated
/// historical bug (e.g. a dismissed, unresolved critical episode followed
/// by a later, distinct critical episode that a naive merge would silently
/// inherit the old dismissal). Timestamp-only heuristics (keep-freshest by
/// `last_observed_at`) cannot distinguish these cases — a historical
/// buggy reconciler could have observed the older row after a newer
/// episode opened, making the older row appear "freshest" and causing the
/// repair to preserve a stale dismissed row over a current undismissed one.
///
/// Instead, ALL duplicate rows for the identity group are resolved, and
/// for categories that support it, a single current occurrence is
/// reconstructed immediately from authoritative domain state:
///
/// * `flakes` — from `sync_status`/`last_sync_at` (see below).
/// * `cves` — from `view_cve_list_with_metadata` using the EXACT same
///   `severity = 'CRITICAL' AND affected_count > 0` predicate the
///   production scan-save path (`cve_scans::save_scan_results_with_store_path_override`)
///   already uses to resolve stale occurrences. This reconstruction is
///   NOT optional the way it is for `systems`/`environments`: unlike
///   those, CVEs have no periodic reconciliation sweep at all — a fresh
///   occurrence is otherwise only produced by the next vulnerability scan,
///   which may run on an arbitrarily distant or effectively absent future
///   schedule for a given target, so skipping reconstruction here could
///   leave a genuinely fleet-relevant CRITICAL CVE's badge silently absent
///   indefinitely rather than for a bounded gap.
///
/// `systems` and `environments` have no domain-level reconstruction here
/// and instead rely on the periodic reconciler (2-minute cycle) to
/// recreate the current occurrence — a brief gap in the badge count after
/// a one-time startup repair is acceptable for those, since that
/// reconciler is guaranteed to run shortly and will recreate them.
///
/// `environments` additionally narrows grouping to
/// `(subject_id, underlying_system_id)` — not also
/// `underlying_system_occurrence_key` — so distinct systems contributing
/// independent, simultaneous incidents to the same environment are never
/// treated as duplicates of each other, while still tolerating a system
/// whose environment membership changed and changed back (A → B → A)
/// without a stable episode-key distinction being required.
///
/// NULL-safe equality (`IS NOT DISTINCT FROM`) is used for the
/// `underlying_system_id` predicate rather than a generic
/// "`$n` IS NULL means match anything" pattern — a NULL
/// `underlying_system_id` on some malformed legacy row must only match
/// *other* rows that are equally NULL, not every row for that environment.
///
/// Safe to call repeatedly and on every server startup — a no-op once at
/// most one open occurrence remains per identity group.
///
/// Callers investigating a suspected pre-existing duplication (e.g. an
/// inflated navigation badge count) should first run:
///
/// ```sql
/// SELECT category, subject_id, COUNT(*) AS occurrence_count,
///        ARRAY_AGG(source_occurrence_key ORDER BY opened_at) AS keys
/// FROM attention_occurrences
/// WHERE resolved_at IS NULL
/// GROUP BY category, subject_id
/// HAVING COUNT(*) > 1
/// ORDER BY occurrence_count DESC;
/// ```
///
/// Also group by `subject_id` alone and count distinct `opened_at` values
/// to distinguish true key-encoding duplicates from legitimately distinct
/// episodes before drawing conclusions about the shape of any observed
/// duplication.
///
/// Canonicalize every attention occurrence for one build job to the single
/// deterministic key `build:<job_id>`.
///
/// Builds are the SAFE-TO-MERGE category (see the module-level doc comment
/// above): the key has no episode/timestamp component, so a build job can
/// never legitimately recur under one `subject_id` — any row sharing that
/// subject_id with a non-canonical key can only be an artifact of a
/// historical key-encoding bug, i.e. provably the same real-world event.
///
/// Round 11 finding: the previous "pick a canonical row among open
/// duplicates" repair only fired when MORE THAN ONE open row existed for a
/// subject_id, so a single malformed open row with no open canonical
/// counterpart was left completely untouched — and would then be JOINED by
/// a second, canonical-keyed row from `reconcile_terminal_events` (which
/// creates `build:<job_id>` whenever that exact key does not currently
/// exist), producing exactly the duplicate this repair exists to prevent.
/// This function instead unconditionally ensures a single open canonical
/// row exists for the given job, handling three possible starting states:
///
/// * The canonical row is already open — nothing to create; only the
///   malformed row(s)' dismissals need migrating and the rows resolving.
/// * No canonical row exists at all — a fresh canonical row is inserted
///   using the EARLIEST malformed row's `opened_at`/`metadata`, so the
///   24-hour attention window reflects the real original failure time,
///   not this repair's run time.
/// * The canonical row exists but is RESOLVED while a malformed row for the
///   SAME job is still open. Since a build job can never legitimately
///   recur under one subject_id, this combination can only be the result
///   of a prior reconciliation bug that incorrectly resolved the canonical
///   row while leaving a duplicate open — inserting a second row with the
///   same `source_occurrence_key` is not possible (the unique constraint
///   on `(category, source_occurrence_key)` would reject it), so the
///   canonical row is REOPENED (`resolved_at = NULL`) instead. This is the
///   one case in the whole attention-lifecycle codebase where an already-
///   resolved row is un-resolved; it is safe here specifically because
///   the malformed companion row proves the underlying failure event was
///   never actually superseded — repairing a mis-resolve is not the same
///   as reopening a genuinely closed incident.
///
/// Returns the number of malformed rows resolved (does not count the
/// canonical row's own insert/reopen as a "resolved" row).
async fn canonicalize_build_occurrence(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    job_id: Uuid,
) -> Result<usize> {
    let subject_id = job_id.to_string();
    let canonical_key = build_occurrence_key(job_id);

    let malformed_open: Vec<Uuid> = sqlx::query_scalar(
        r#"
        SELECT id FROM attention_occurrences
        WHERE category = 'builds' AND subject_id = $1 AND resolved_at IS NULL
          AND source_occurrence_key <> $2
        FOR UPDATE
        "#,
    )
    .bind(&subject_id)
    .bind(&canonical_key)
    .fetch_all(&mut **tx)
    .await
    .context("failed to find malformed open build occurrences")?;

    if malformed_open.is_empty() {
        return Ok(0);
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Round 12: check authoritative build status BEFORE canonicalizing.
    // A malformed open occurrence does NOT prove the build is still failed —
    // the build may have succeeded after the malformed row was created, and
    // the domain transition's best-effort attention resolution may have been
    // lost to a crash. Reopening a resolved canonical row (or creating a new
    // canonical row from scratch) for a non-failed build would falsely
    // represent a successful build as requiring attention.
    //
    // Rules:
    //   - build missing or status != 'failed' → resolve every open occurrence
    //     for this subject (malformed AND canonical). Do not reopen or create.
    //   - status = 'failed' → proceed with the existing canonicalization logic:
    //     canonical key is ensured open, dismissals migrated, malformed rows
    //     resolved. Use `completed_at` as the event timestamp.
    // ═══════════════════════════════════════════════════════════════════════
    let build_state: Option<(String, Option<DateTime<Utc>>)> = sqlx::query_as(
        r#"
        SELECT status, completed_at FROM build_jobs WHERE id = $1
        "#,
    )
    .bind(job_id)
    .fetch_optional(&mut **tx)
    .await
    .context("failed to read build state for canonicalization")?;

    let should_canonicalize = match &build_state {
        Some((status, _)) if status == "failed" => true,
        _ => false,
    };

    if !should_canonicalize {
        // Build is missing or not failed — resolve every open occurrence for
        // this subject (malformed rows AND any open canonical row) rather than
        // creating or reopening anything.
        let resolved = sqlx::query(
            "UPDATE attention_occurrences SET resolved_at = NOW() \
             WHERE category = 'builds' AND subject_id = $1 AND resolved_at IS NULL",
        )
        .bind(&subject_id)
        .execute(&mut **tx)
        .await
        .context("failed to resolve build occurrences for non-failed build")?;
        // Also remove any malformed-open dismissals that would be orphaned
        // by deletion (no canonical row exists to hold them), since the
        // malformed rows will now be resolved.
        return Ok(resolved.rows_affected() as usize);
    }

    let completed_at = build_state
        .as_ref()
        .and_then(|(_, c)| *c)
        .unwrap_or_else(Utc::now);

    let canonical_row: Option<(Uuid, Option<DateTime<Utc>>)> = sqlx::query_as(
        r#"
        SELECT id, resolved_at FROM attention_occurrences
        WHERE category = 'builds' AND subject_id = $1 AND source_occurrence_key = $2
        FOR UPDATE
        "#,
    )
    .bind(&subject_id)
    .bind(&canonical_key)
    .fetch_optional(&mut **tx)
    .await
    .context("failed to find canonical build occurrence")?;

    let canonical_id = match canonical_row {
        Some((id, None)) => id,
        Some((id, Some(_))) => {
            // Resolved canonical row + open malformed row for the same job
            // AND the build is still failed: reopening is safe here because
            // the malformed companion row proves the underlying failure event
            // was never actually superseded — repairing a mis-resolve is not
            // the same as reopening a genuinely closed incident. Use the
            // build's `completed_at` as the event timestamp.
            sqlx::query(
                "UPDATE attention_occurrences \
                 SET resolved_at = NULL, opened_at = $1, last_observed_at = GREATEST(last_observed_at, $1) \
                 WHERE id = $2",
            )
            .bind(completed_at)
            .bind(id)
            .execute(&mut **tx)
            .await
            .context("failed to reopen canonical build occurrence")?;
            id
        }
        None => {
            // Use the earliest malformed row's metadata for diagnostic info,
            // but use `build_jobs.completed_at` as the event timestamp so
            // the 24-hour attention window reflects the actual failure time.
            let metadata: serde_json::Value = sqlx::query_scalar(
                r#"
                SELECT metadata FROM attention_occurrences
                WHERE id = ANY($1)
                ORDER BY opened_at ASC
                LIMIT 1
                "#,
            )
            .bind(&malformed_open)
            .fetch_one(&mut **tx)
            .await
            .context("failed to read malformed build occurrence metadata for canonicalization")?;

            sqlx::query_scalar::<_, Uuid>(
                r#"
                INSERT INTO attention_occurrences (
                    category, subject_type, subject_id, source_occurrence_key,
                    opened_at, last_observed_at, metadata
                )
                VALUES ('builds', 'build_job', $1, $2, $3, $3, $4)
                RETURNING id
                "#,
            )
            .bind(&subject_id)
            .bind(&canonical_key)
            .bind(completed_at)
            .bind(&metadata)
            .fetch_one(&mut **tx)
            .await
            .context("failed to insert canonical build occurrence")?
        }
    };

    // Migrate dismissals from every malformed row onto the canonical row.
    sqlx::query(
        r#"
        INSERT INTO user_attention_dismissals (user_id, occurrence_id, dismissed_at)
        SELECT uad.user_id, $1, MIN(uad.dismissed_at)
        FROM user_attention_dismissals uad
        WHERE uad.occurrence_id = ANY($2)
        GROUP BY uad.user_id
        ON CONFLICT (user_id, occurrence_id) DO NOTHING
        "#,
    )
    .bind(canonical_id)
    .bind(&malformed_open)
    .execute(&mut **tx)
    .await
    .context("failed to migrate build dismissals during canonicalization")?;

    // Resolve every malformed row.
    let result = sqlx::query(
        "UPDATE attention_occurrences SET resolved_at = NOW() \
         WHERE id = ANY($1) AND resolved_at IS NULL",
    )
    .bind(&malformed_open)
    .execute(&mut **tx)
    .await
    .context("failed to resolve malformed build occurrences during canonicalization")?;

    Ok(result.rows_affected() as usize)
}

/// Find every build job with at least one OPEN, non-canonical-key attention
/// occurrence and canonicalize it via [`canonicalize_build_occurrence`].
///
/// Unlike the generic duplicate-group repair in [`dedupe_open_occurrences`]
/// (which only examines subject_ids with MORE THAN ONE open row), this also
/// catches a single malformed open row with no open canonical counterpart —
/// see [`canonicalize_build_occurrence`]'s doc comment for why that case
/// matters. Run this BEFORE computing duplicate groups in
/// `dedupe_open_occurrences`, so that by the time the group query runs,
/// builds have already been reduced to at most one open row per subject_id.
async fn canonicalize_malformed_build_occurrences(
    tx: &mut sqlx::Transaction<'_, Postgres>,
) -> Result<usize> {
    let subjects: Vec<String> = sqlx::query_scalar(
        r#"
        SELECT DISTINCT subject_id FROM attention_occurrences
        WHERE category = 'builds'
          AND resolved_at IS NULL
          AND source_occurrence_key <> ('build:' || subject_id)
        "#,
    )
    .fetch_all(&mut **tx)
    .await
    .context("failed to find malformed build subjects")?;

    let mut total = 0usize;
    for subject_id in subjects {
        if let Ok(job_id) = subject_id.parse::<Uuid>() {
            total += canonicalize_build_occurrence(tx, job_id).await?;
        }
    }
    Ok(total)
}

pub async fn dedupe_open_occurrences(pool: &PgPool) -> Result<usize> {
    let mut tx = pool
        .begin()
        .await
        .context("failed to begin dedupe transaction")?;

    // Canonicalize malformed build occurrences FIRST, before computing
    // duplicate groups below — see `canonicalize_malformed_build_occurrences`'s
    // doc comment for why this must run as a separate, earlier pass rather
    // than solely relying on the duplicate-group query.
    let mut total_resolved = canonicalize_malformed_build_occurrences(&mut tx).await?;

    // `extra` narrows identity beyond (category, subject_id):
    //   evals        -> opened_at (as text) -- exact-event identity
    //   environments -> underlying_system_id -- keeps distinct systems'
    //                   incidents in the same environment from ever being
    //                   considered duplicates of each other
    //   everything else -> NULL (grouping is (category, subject_id) only)
    let groups: Vec<(String, String, Option<String>)> = sqlx::query_as(
        r#"
        SELECT category, subject_id, extra FROM (
            SELECT category, subject_id,
                   CASE
                       WHEN category = 'evals' THEN opened_at::text
                       WHEN category = 'environments' THEN metadata->>'underlying_system_id'
                       ELSE NULL
                   END AS extra
            FROM attention_occurrences
            WHERE resolved_at IS NULL
        ) grouped
        GROUP BY category, subject_id, extra
        HAVING COUNT(*) > 1
        "#,
    )
    .fetch_all(&mut *tx)
    .await
    .context("failed to find duplicate open occurrence groups")?;

    for (category, subject_id, extra) in groups {
        if category == "evals" {
            let canonical_id: Option<Uuid> = sqlx::query_scalar(
                r#"
                SELECT id FROM attention_occurrences
                WHERE category = 'evals' AND subject_id = $1 AND resolved_at IS NULL
                  AND opened_at = $2::timestamptz
                ORDER BY id ASC
                LIMIT 1
                FOR UPDATE
                "#,
            )
            .bind(&subject_id)
            .bind(&extra)
            .fetch_optional(&mut *tx)
            .await
            .context("failed to find canonical eval occurrence")?;

            let Some(canonical_id) = canonical_id else {
                continue;
            };

            sqlx::query(
                r#"
                INSERT INTO user_attention_dismissals (user_id, occurrence_id, dismissed_at)
                SELECT uad.user_id, $3, MIN(uad.dismissed_at)
                FROM user_attention_dismissals uad
                JOIN attention_occurrences ao ON ao.id = uad.occurrence_id
                WHERE ao.category = 'evals' AND ao.subject_id = $1 AND ao.resolved_at IS NULL
                  AND ao.opened_at = $2::timestamptz
                  AND ao.id <> $3
                GROUP BY uad.user_id
                ON CONFLICT (user_id, occurrence_id) DO NOTHING
                "#,
            )
            .bind(&subject_id)
            .bind(&extra)
            .bind(canonical_id)
            .execute(&mut *tx)
            .await
            .context("failed to migrate eval dismissals during dedupe")?;

            let result = sqlx::query(
                r#"
                UPDATE attention_occurrences
                SET resolved_at = NOW()
                WHERE category = 'evals' AND subject_id = $1 AND resolved_at IS NULL
                  AND opened_at = $2::timestamptz
                  AND id <> $3
                "#,
            )
            .bind(&subject_id)
            .bind(&extra)
            .bind(canonical_id)
            .execute(&mut *tx)
            .await
            .context("failed to resolve duplicate eval occurrences during dedupe")?;
            total_resolved += result.rows_affected() as usize;
        } else if category == "builds" {
            // Defense-in-depth: `canonicalize_malformed_build_occurrences`
            // already ran above, before this groups query, and should have
            // reduced every build subject_id to at most one open row — so
            // this branch should not normally fire (a "builds" group here
            // implies more than one row is STILL open for this subject_id
            // after that pass, which should not happen in practice). If it
            // ever does, route it through the exact same canonicalization
            // logic rather than a separate ad hoc merge, so the same
            // canonical-key/reopen/dismissal-migration guarantees apply.
            let Ok(job_id) = subject_id.parse::<Uuid>() else {
                continue;
            };
            total_resolved += canonicalize_build_occurrence(&mut tx, job_id).await?;
        } else if category == "environments" {
            // Resolve ALL duplicates — do not use timestamp heuristics to
            // pick a "canonical" one, as last_observed_at can be misleading
            // (a historical reconciler may have observed the older row after
            // a newer episode opened, causing the repair to keep the older,
            // possibly dismissed row and resolve the current undismissed row).
            // Environment occurrences are reconstructed by the next periodic
            // system-reconciliation sweep (within 2 minutes).
            let result = sqlx::query(
                r#"
                UPDATE attention_occurrences
                SET resolved_at = NOW()
                WHERE category = 'environments' AND subject_id = $1 AND resolved_at IS NULL
                  AND (metadata->>'underlying_system_id') IS NOT DISTINCT FROM $2
                "#,
            )
            .bind(&subject_id)
            .bind(&extra)
            .execute(&mut *tx)
            .await
            .context("failed to resolve duplicate environment occurrences during dedupe")?;
            total_resolved += result.rows_affected() as usize;
        } else if category == "cves" {
            // Round 16: route all CVE attention transitions through the
            // canonical per-CVE lock helper.  The helper resolves any
            // duplicate occurrences and re-opens a single canonical one when
            // the CVE is still fleet-relevant, preserving the earliest
            // opened_at and updating cves.fleet_relevant_since.  Because it is
            // called in the dedupe transaction, we use the in-transaction
            // variant.
            reconcile_cve_attention_subject_tx(&mut tx, &subject_id)
                .await
                .context("failed to reconcile CVE attention during dedupe")?;

            // Count the duplicates that were resolved; the helper resolves
            // every open CVE row for this subject in a single UPDATE.
            // We do not count the possibly-reinserted canonical row here.
            // (total_resolved tracks resolved duplicates, not net rows.)
        } else {
            // Covers systems, flakes, and any future/unlisted category by
            // default: resolve ALL duplicates without reconstruction, since
            // each has its own periodic sweep to recreate the occurrence if
            // still relevant.
            let resolved = sqlx::query(
                r#"
                UPDATE attention_occurrences
                SET resolved_at = NOW()
                WHERE category = $1 AND subject_id = $2 AND resolved_at IS NULL
                "#,
            )
            .bind(&category)
            .bind(&subject_id)
            .execute(&mut *tx)
            .await
            .context("failed to resolve duplicate occurrences during dedupe")?;
            total_resolved += resolved.rows_affected() as usize;

            // Reconstruct flake attention from authoritative sync status.
            if category == "flakes" {
                let flake_id: i32 = match subject_id.parse() {
                    Ok(id) => id,
                    Err(_) => continue,
                };
                let state: Option<(String, Option<DateTime<Utc>>, Option<String>)> =
                    match sqlx::query_as(
                        "SELECT sync_status, last_sync_at, last_sync_error \
                         FROM flakes WHERE id = $1 AND deleted_at IS NULL",
                    )
                    .bind(flake_id)
                    .fetch_optional(&mut *tx)
                    .await
                    {
                        Ok(v) => v,
                        Err(e) => {
                            warn!("failed to query flake state for dedupe reconstruction: {e:#}");
                            continue;
                        }
                    };

                if let Some((sync_status, last_sync_at, last_sync_error)) = state {
                    let now = Utc::now();
                    if sync_status == "error" {
                        let episode_id = Uuid::new_v4();
                        let source_key = flake_occurrence_key(flake_id, episode_id);
                        let metadata = serde_json::json!({
                            "reason": "sync_error",
                            "flake_id": flake_id,
                            "last_sync_error": last_sync_error,
                        });
                        if let Err(e) = sqlx::query(
                            r#"
                            INSERT INTO attention_occurrences (
                                category, subject_type, subject_id, source_occurrence_key,
                                opened_at, last_observed_at, metadata
                            )
                            VALUES ('flakes', 'flake_sync', $1, $2, $3, $4, $5)
                            "#,
                        )
                        .bind(&subject_id)
                        .bind(source_key)
                        .bind(last_sync_at.unwrap_or(now))
                        .bind(now)
                        .bind(&metadata)
                        .execute(&mut *tx)
                        .await
                        {
                            warn!("failed to reconstruct error occurrence for flake {flake_id}: {e:#}");
                        }
                    } else if sync_status == "syncing" {
                        if let Some(sync_at) = last_sync_at {
                            if sync_at < now - Duration::minutes(30) {
                                let episode_id = Uuid::new_v4();
                                let source_key = flake_occurrence_key(flake_id, episode_id);
                                let opened_at = sync_at + Duration::minutes(30);
                                let metadata = serde_json::json!({
                                    "reason": "stale_sync",
                                    "flake_id": flake_id,
                                });
                                if let Err(e) = sqlx::query(
                                    r#"
                                    INSERT INTO attention_occurrences (
                                        category, subject_type, subject_id, source_occurrence_key,
                                        opened_at, last_observed_at, metadata
                                    )
                                    VALUES ('flakes', 'flake_sync', $1, $2, $3, $4, $5)
                                    "#,
                                )
                                .bind(&subject_id)
                                .bind(source_key)
                                .bind(opened_at)
                                .bind(now)
                                .bind(&metadata)
                                .execute(&mut *tx)
                                .await
                                {
                                    warn!("failed to reconstruct stale-sync occurrence for flake {flake_id}: {e:#}");
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    tx.commit()
        .await
        .context("failed to commit dedupe transaction")?;

    Ok(total_resolved)
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
        assert!(!is_attention_eligible(
            cutoff - Duration::microseconds(1),
            observed
        ));
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

    // ── Live-database lifecycle tests ───────────────────────────────────────
    //
    // These exercise the dynamic SQL paths that cannot be caught by
    // `cargo check` under `SQLX_OFFLINE=true` (runtime `sqlx::query`, not
    // `sqlx::query!`). Run against a repository-provided isolated database:
    //   DATABASE_URL=postgres://crystal_forge:password@localhost:3042/crystal_forge \
    //     cargo test -p cf-server --lib queries::attention -- --ignored

    async fn test_pool() -> PgPool {
        PgPool::connect(
            &std::env::var("DATABASE_URL").expect("DATABASE_URL must be set for DB tests"),
        )
        .await
        .expect("failed to connect to test database")
    }

    async fn insert_throwaway_user(pool: &PgPool) -> Uuid {
        let user_id = Uuid::new_v4();
        let short = user_id.simple().to_string()[..12].to_string();
        sqlx::query(
            "INSERT INTO users (id, username, first_name, last_name, email, user_type) \
             VALUES ($1, $2, 'Test', 'User', $3, 'human')",
        )
        .bind(user_id)
        .bind(format!("att-{short}"))
        .bind(format!("att-{short}@example.com"))
        .execute(pool)
        .await
        .expect("failed to insert throwaway test user");
        user_id
    }

    async fn insert_throwaway_environment(pool: &PgPool, name_hint: &str) -> Uuid {
        let env_id = Uuid::new_v4();
        sqlx::query("INSERT INTO environments (id, name) VALUES ($1, $2)")
            .bind(env_id)
            .bind(format!(
                "att-{name_hint}-{}",
                &env_id.simple().to_string()[..8]
            ))
            .execute(pool)
            .await
            .expect("failed to insert throwaway test environment");
        env_id
    }

    async fn insert_throwaway_system(pool: &PgPool, environment_id: Uuid) -> Uuid {
        let system_id = Uuid::new_v4();
        let short = system_id.simple().to_string()[..12].to_string();
        sqlx::query(
            "INSERT INTO systems (id, hostname, environment_id, is_active, public_key, derivation) \
             VALUES ($1, $2, $3, TRUE, $4, $5)",
        )
        .bind(system_id)
        .bind(format!("att-sys-{short}"))
        .bind(environment_id)
        .bind(format!("ssh-ed25519 AAAA-test-{short}"))
        .bind("/nix/store/att-test-derivation")
        .execute(pool)
        .await
        .expect("failed to insert throwaway test system");
        system_id
    }

    async fn cleanup_user(pool: &PgPool, user_id: Uuid) {
        let _ = sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(user_id)
            .execute(pool)
            .await;
    }

    async fn cleanup_environment(pool: &PgPool, environment_id: Uuid) {
        // systems.environment_id has a FK to environments with no cascade,
        // so any throwaway systems must be deleted first or this silently
        // no-ops (via the `let _ =` below) and leaks both rows.
        let _ = sqlx::query("DELETE FROM systems WHERE environment_id = $1")
            .bind(environment_id)
            .execute(pool)
            .await;
        let _ = sqlx::query("DELETE FROM environments WHERE id = $1")
            .bind(environment_id)
            .execute(pool)
            .await;
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn test_open_or_observe_by_subject_opens_then_observes() {
        let pool = test_pool().await;
        let subject_id = Uuid::new_v4().to_string();

        let first = open_or_observe_by_subject(
            &pool,
            "flakes",
            "flake_sync",
            &subject_id,
            "sync_error",
            Utc::now(),
            serde_json::json!({}),
            |reason, episode_id| format!("flake:{subject_id}:{reason}:{episode_id}"),
        )
        .await
        .expect("first open should succeed");

        let second = open_or_observe_by_subject(
            &pool,
            "flakes",
            "flake_sync",
            &subject_id,
            "sync_error",
            Utc::now(),
            serde_json::json!({}),
            |reason, episode_id| format!("flake:{subject_id}:{reason}:{episode_id}"),
        )
        .await
        .expect("second call should observe the existing occurrence");

        assert_eq!(
            first, second,
            "same subject+reason must converge to one occurrence"
        );

        let open_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM attention_occurrences WHERE category = 'flakes' AND subject_id = $1 AND resolved_at IS NULL",
        )
        .bind(&subject_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(open_count, 1);

        let _ = sqlx::query("DELETE FROM attention_occurrences WHERE subject_id = $1")
            .bind(&subject_id)
            .execute(&pool)
            .await;
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn test_open_or_observe_by_subject_concurrent_calls_converge_to_one_occurrence() {
        // Regression test for the race where two concurrent callers could
        // both observe "no open row" and each insert a distinct, randomly
        // keyed episode. The transaction-scoped advisory lock in
        // open_or_observe_by_subject must serialize these.
        let pool = test_pool().await;
        let subject_id = Uuid::new_v4().to_string();

        let mut handles = Vec::new();
        for _ in 0..8 {
            let pool = pool.clone();
            let subject_id = subject_id.clone();
            handles.push(tokio::spawn(async move {
                open_or_observe_by_subject(
                    &pool,
                    "systems",
                    "system_health",
                    &subject_id,
                    "critical",
                    Utc::now(),
                    serde_json::json!({}),
                    |reason, episode_id| format!("system:{subject_id}:{reason}:{episode_id}"),
                )
                .await
            }));
        }

        let mut ids = Vec::new();
        for h in handles {
            ids.push(h.await.unwrap().expect("open_or_observe_by_subject failed"));
        }

        let distinct: std::collections::HashSet<_> = ids.into_iter().collect();
        assert_eq!(
            distinct.len(),
            1,
            "concurrent calls for the same (category, subject, reason) must converge to exactly one occurrence"
        );

        let open_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM attention_occurrences WHERE category = 'systems' AND subject_id = $1 AND resolved_at IS NULL",
        )
        .bind(&subject_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(open_count, 1);

        let _ = sqlx::query("DELETE FROM attention_occurrences WHERE subject_id = $1")
            .bind(&subject_id)
            .execute(&pool)
            .await;
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn test_resolve_actually_resolves_by_subject_type_and_id() {
        // Regression test: resolve() previously referenced nonexistent
        // source_kind/source_id columns and silently resolved zero rows.
        let pool = test_pool().await;
        let subject_id = Uuid::new_v4().to_string();
        let key = format!("build:{subject_id}");

        open_or_observe(
            &pool,
            "builds",
            "build_job",
            &subject_id,
            &key,
            Utc::now(),
            serde_json::json!({}),
        )
        .await
        .expect("open should succeed");

        let affected = resolve(&pool, "builds", "build_job", &subject_id)
            .await
            .expect("resolve should succeed");
        assert_eq!(
            affected, 1,
            "resolve() must actually resolve the matching row"
        );

        let still_open: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM attention_occurrences WHERE category = 'builds' AND subject_id = $1 AND resolved_at IS NULL",
        )
        .bind(&subject_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(still_open, 0);

        let _ = sqlx::query("DELETE FROM attention_occurrences WHERE subject_id = $1")
            .bind(&subject_id)
            .execute(&pool)
            .await;
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn test_critical_to_offline_transition_leaves_exactly_one_open_occurrence() {
        let pool = test_pool().await;
        let subject_id = Uuid::new_v4().to_string();

        open_or_observe_by_subject(
            &pool,
            "systems",
            "system_health",
            &subject_id,
            "critical",
            Utc::now(),
            serde_json::json!({}),
            |reason, episode_id| format!("system:{subject_id}:{reason}:{episode_id}"),
        )
        .await
        .expect("open critical should succeed");

        // Simulate the critical -> offline transition the way the
        // reconciler does: resolve the old reason family, then open/observe
        // the new one.
        resolve_open_occurrences_except_reason(&pool, "systems", &subject_id, "offline")
            .await
            .expect("resolve except reason should succeed");

        open_or_observe_by_subject(
            &pool,
            "systems",
            "system_health",
            &subject_id,
            "offline",
            Utc::now(),
            serde_json::json!({}),
            |reason, episode_id| format!("system:{subject_id}:{reason}:{episode_id}"),
        )
        .await
        .expect("open offline should succeed");

        let open_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM attention_occurrences WHERE category = 'systems' AND subject_id = $1 AND resolved_at IS NULL",
        )
        .bind(&subject_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            open_count, 1,
            "a critical -> offline transition must not leave two simultaneously-open occurrences"
        );

        let total_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM attention_occurrences WHERE category = 'systems' AND subject_id = $1",
        )
        .bind(&subject_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            total_count, 2,
            "the critical episode should still exist, resolved"
        );

        let _ = sqlx::query("DELETE FROM attention_occurrences WHERE subject_id = $1")
            .bind(&subject_id)
            .execute(&pool)
            .await;
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn test_dismiss_occurrences_rejects_out_of_scope_system() {
        let pool = test_pool().await;
        let user_id = insert_throwaway_user(&pool).await;
        let other_env_id = insert_throwaway_environment(&pool, "other").await;
        let system_id = insert_throwaway_system(&pool, other_env_id).await;

        let subject_id = system_id.to_string();
        let key = format!("system:{subject_id}:critical:{}", Uuid::new_v4());
        let opened_at = Utc::now();
        open_or_observe(
            &pool,
            "systems",
            "system_health",
            &subject_id,
            &key,
            opened_at,
            serde_json::json!({}),
        )
        .await
        .expect("open should succeed");

        // Non-admin user with NO membership in other_env_id must not be able
        // to dismiss an occurrence scoped to that environment.
        let result = dismiss_occurrences(
            &pool,
            user_id,
            "systems",
            opened_at,
            &[key.clone()],
            false,
            &[],
        )
        .await;
        assert!(
            result.is_err(),
            "dismissal of an out-of-scope system occurrence must be rejected"
        );

        let dismissed: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM user_attention_dismissals uad \
             JOIN attention_occurrences ao ON ao.id = uad.occurrence_id \
             WHERE ao.source_occurrence_key = $1",
        )
        .bind(&key)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(dismissed, 0, "no dismissal row should have been inserted");

        let _ = sqlx::query("DELETE FROM attention_occurrences WHERE subject_id = $1")
            .bind(&subject_id)
            .execute(&pool)
            .await;
        cleanup_user(&pool, user_id).await;
        cleanup_environment(&pool, other_env_id).await;
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn test_dismiss_occurrences_allows_in_scope_system() {
        let pool = test_pool().await;
        let user_id = insert_throwaway_user(&pool).await;
        let member_env_id = insert_throwaway_environment(&pool, "member").await;
        let system_id = insert_throwaway_system(&pool, member_env_id).await;

        let subject_id = system_id.to_string();
        let key = format!("system:{subject_id}:critical:{}", Uuid::new_v4());
        let opened_at = Utc::now();
        open_or_observe(
            &pool,
            "systems",
            "system_health",
            &subject_id,
            &key,
            opened_at,
            serde_json::json!({}),
        )
        .await
        .expect("open should succeed");

        let result = dismiss_occurrences(
            &pool,
            user_id,
            "systems",
            opened_at,
            &[key.clone()],
            false,
            &[member_env_id],
        )
        .await;
        assert!(
            result.is_ok(),
            "dismissal of an in-scope system occurrence must succeed: {:?}",
            result.err()
        );

        let dismissed: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM user_attention_dismissals uad \
             JOIN attention_occurrences ao ON ao.id = uad.occurrence_id \
             WHERE ao.source_occurrence_key = $1 AND uad.user_id = $2",
        )
        .bind(&key)
        .bind(user_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(dismissed, 1);

        let _ = sqlx::query("DELETE FROM attention_occurrences WHERE subject_id = $1")
            .bind(&subject_id)
            .execute(&pool)
            .await;
        cleanup_user(&pool, user_id).await;
        cleanup_environment(&pool, member_env_id).await;
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn test_cleanup_deletes_only_old_resolved_occurrences() {
        let pool = test_pool().await;
        let old_id = Uuid::new_v4().to_string();
        let recent_id = Uuid::new_v4().to_string();

        sqlx::query(
            "INSERT INTO attention_occurrences (id, category, subject_type, subject_id, source_occurrence_key, opened_at, last_observed_at, resolved_at) \
             VALUES (gen_random_uuid(), 'builds', 'build_job', $1, $2, now() - interval '40 days', now() - interval '40 days', now() - interval '35 days')",
        )
        .bind(&old_id)
        .bind(format!("build:{old_id}"))
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO attention_occurrences (id, category, subject_type, subject_id, source_occurrence_key, opened_at, last_observed_at, resolved_at) \
             VALUES (gen_random_uuid(), 'builds', 'build_job', $1, $2, now() - interval '2 days', now() - interval '2 days', now() - interval '1 days')",
        )
        .bind(&recent_id)
        .bind(format!("build:{recent_id}"))
        .execute(&pool)
        .await
        .unwrap();

        let (deleted_occ, _deleted_dis) = cleanup(&pool, Duration::days(30), 1000)
            .await
            .expect("cleanup should succeed");
        assert!(
            deleted_occ >= 1,
            "cleanup must delete at least the old resolved occurrence"
        );

        let old_remains: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM attention_occurrences WHERE subject_id = $1")
                .bind(&old_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(
            old_remains, 0,
            "the old resolved occurrence must be deleted"
        );

        let recent_remains: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM attention_occurrences WHERE subject_id = $1")
                .bind(&recent_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(
            recent_remains, 1,
            "the recently resolved occurrence must be kept"
        );

        let _ = sqlx::query("DELETE FROM attention_occurrences WHERE subject_id = $1")
            .bind(&recent_id)
            .execute(&pool)
            .await;
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn test_dedupe_open_occurrences_keeps_earliest_and_migrates_dismissals() {
        // Uses `builds` deliberately: this test exercises build
        // canonicalization when NEITHER duplicate row uses the canonical
        // `build:<job_id>` key (both are malformed, e.g. from a historical
        // key-encoding bug) and no canonical row exists at all. Round 11:
        // canonicalize_build_occurrence no longer "keeps" either malformed
        // row open — it creates a FRESH canonical-keyed row (using the
        // earliest malformed row's opened_at/metadata) and resolves BOTH
        // malformed rows, migrating dismissals from both onto the new row.
        let pool = test_pool().await;
        let user_id = insert_throwaway_user(&pool).await;
        let job_id = Uuid::new_v4();
        let subject_id = job_id.to_string();
        let canonical_key = format!("build:{subject_id}");

        // Round 12: build_jobs must exist with status = 'failed' for canonicalization.
        let dedupe_short = Uuid::new_v4().simple().to_string()[..12].to_string();
        let derivation_id: i32 = sqlx::query_scalar(
            "INSERT INTO derivations (derivation_type, derivation_name, status_id, attempt_count) \
             VALUES ('package', $1, 11, 0) RETURNING id",
        )
        .bind(format!("test-dedupe-open-{dedupe_short}"))
        .fetch_one(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO build_jobs (id, derivation_id, status, completed_at) \
             VALUES ($1, $2, 'failed', now())",
        )
        .bind(job_id)
        .bind(derivation_id)
        .execute(&pool)
        .await
        .unwrap();

        // Two malformed open occurrences for the same (category, subject_id)
        // — the exact invariant violation this function repairs. Insert the
        // "newer" one first so ordering in the table doesn't accidentally
        // make the test pass regardless of the ORDER BY.
        let newer_id = Uuid::new_v4();
        let older_id = Uuid::new_v4();
        let newer_key = format!("builds_dedupe_test:{subject_id}:newer");
        let older_key = format!("builds_dedupe_test:{subject_id}:older");

        sqlx::query(
            "INSERT INTO attention_occurrences (id, category, subject_type, subject_id, source_occurrence_key, opened_at, last_observed_at) \
             VALUES ($1, 'builds', 'build_job', $2, $3, now(), now())",
        )
        .bind(newer_id)
        .bind(&subject_id)
        .bind(&newer_key)
        .execute(&pool)
        .await
        .expect("insert newer duplicate should succeed");

        sqlx::query(
            "INSERT INTO attention_occurrences (id, category, subject_type, subject_id, source_occurrence_key, opened_at, last_observed_at) \
             VALUES ($1, 'builds', 'build_job', $2, $3, now() - interval '1 hour', now() - interval '1 hour')",
        )
        .bind(older_id)
        .bind(&subject_id)
        .bind(&older_key)
        .execute(&pool)
        .await
        .expect("insert older duplicate should succeed");

        // A user dismissed the newer (non-canonical) duplicate.
        sqlx::query(
            "INSERT INTO user_attention_dismissals (user_id, occurrence_id, dismissed_at) VALUES ($1, $2, NOW())",
        )
        .bind(user_id)
        .bind(newer_id)
        .execute(&pool)
        .await
        .expect("dismiss newer duplicate should succeed");

        let resolved = dedupe_open_occurrences(&pool)
            .await
            .expect("dedupe should succeed");
        assert_eq!(
            resolved, 2,
            "both malformed rows must be resolved (neither is kept as canonical)"
        );

        // Both original malformed rows must now be resolved.
        let older_resolved: Option<chrono::DateTime<Utc>> = sqlx::query_scalar(
            "SELECT resolved_at FROM attention_occurrences WHERE id = $1",
        )
        .bind(older_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(older_resolved.is_some(), "the older malformed row must be resolved");

        let newer_resolved: Option<chrono::DateTime<Utc>> = sqlx::query_scalar(
            "SELECT resolved_at FROM attention_occurrences WHERE id = $1",
        )
        .bind(newer_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(newer_resolved.is_some(), "the newer malformed row must be resolved");

        // A NEW canonical-keyed row must now exist, open, using the
        // EARLIEST malformed row's opened_at.
        let canonical_row: Option<(Uuid, Option<chrono::DateTime<Utc>>, chrono::DateTime<Utc>)> =
            sqlx::query_as(
                "SELECT id, resolved_at, opened_at FROM attention_occurrences \
                 WHERE category = 'builds' AND subject_id = $1 AND source_occurrence_key = $2",
            )
            .bind(&subject_id)
            .bind(&canonical_key)
            .fetch_optional(&pool)
            .await
            .unwrap();
        let (canonical_id, canonical_resolved, canonical_opened_at) =
            canonical_row.expect("a canonical build:<job_id> row must have been created");
        assert!(
            canonical_resolved.is_none(),
            "the newly created canonical row must be open"
        );
        // Round 12: the canonical row's opened_at uses build_jobs.completed_at
        // as the authoritative event timestamp, not the malformed row's opened_at.
        let bj_completed_at: chrono::DateTime<Utc> = sqlx::query_scalar(
            "SELECT completed_at FROM build_jobs WHERE id = $1",
        )
        .bind(job_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            canonical_opened_at, bj_completed_at,
            "the canonical row's opened_at must match build_jobs.completed_at, \
             not the malformed row's opened_at"
        );

        // The user's dismissal must have migrated to the NEW canonical row.
        let migrated: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM user_attention_dismissals WHERE user_id = $1 AND occurrence_id = $2",
        )
        .bind(user_id)
        .bind(canonical_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            migrated, 1,
            "dismissal must be migrated to the new canonical occurrence"
        );

        // Running dedupe again must be a no-op (idempotent).
        let resolved_again = dedupe_open_occurrences(&pool)
            .await
            .expect("second dedupe run should succeed");
        assert_eq!(
            resolved_again, 0,
            "dedupe must be a no-op once at most one open occurrence remains"
        );

        let _ = sqlx::query("DELETE FROM attention_occurrences WHERE subject_id = $1")
            .bind(&subject_id)
            .execute(&pool)
            .await;
        let _ = sqlx::query("DELETE FROM build_jobs WHERE id = $1")
            .bind(job_id)
            .execute(&pool)
            .await;
        let _ = sqlx::query("DELETE FROM derivations WHERE id = $1")
            .bind(derivation_id)
            .execute(&pool)
            .await;
        cleanup_user(&pool, user_id).await;
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn test_canonicalize_build_single_malformed_row_no_canonical() {
        // Regression test for round 11: a SINGLE malformed open row (no
        // duplicate, no canonical row at all) was previously invisible to
        // dedup entirely -- the duplicate-group query's `HAVING COUNT(*) > 1`
        // filter excludes any subject_id with only one open row. This left
        // the malformed row unrepaired, and terminal reconciliation would
        // then create a SECOND, canonical-keyed row for the same job
        // (since `build:<job_id>` does not exist), producing exactly the
        // duplicate this repair exists to prevent.
        //
        // Round 12: the canonicalizer now checks `build_jobs.status` before
        // acting. A build_job must exist AND be 'failed' for canonicalization
        // to proceed; otherwise all open occurrences are resolved without
        // creating a canonical replacement (the build either succeeded or
        // never existed).
        let pool = test_pool().await;
        let user_id = insert_throwaway_user(&pool).await;
        let job_id = Uuid::new_v4();
        let subject_id = job_id.to_string();
        let canonical_key = format!("build:{subject_id}");

        // Build job must exist with status = 'failed' for canonicalization.
        let short_single = Uuid::new_v4().simple().to_string()[..12].to_string();
        let derivation_id: i32 = sqlx::query_scalar(
            "INSERT INTO derivations (derivation_type, derivation_name, status_id, attempt_count) \
             VALUES ('package', $1, 11, 0) RETURNING id",
        )
        .bind(format!("test-canon-single-{short_single}"))
        .fetch_one(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO build_jobs (id, derivation_id, status, completed_at) \
             VALUES ($1, $2, 'failed', now())",
        )
        .bind(job_id)
        .bind(derivation_id)
        .execute(&pool)
        .await
        .unwrap();

        let malformed_id = Uuid::new_v4();
        let malformed_key = format!("builds_malformed_test:{subject_id}");

        sqlx::query(
            "INSERT INTO attention_occurrences (id, category, subject_type, subject_id, source_occurrence_key, opened_at, last_observed_at) \
             VALUES ($1, 'builds', 'build_job', $2, $3, now() - interval '2 hours', now() - interval '2 hours')",
        )
        .bind(malformed_id)
        .bind(&subject_id)
        .bind(&malformed_key)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO user_attention_dismissals (user_id, occurrence_id, dismissed_at) VALUES ($1, $2, NOW())",
        )
        .bind(user_id)
        .bind(malformed_id)
        .execute(&pool)
        .await
        .unwrap();

        let resolved = dedupe_open_occurrences(&pool)
            .await
            .expect("dedupe should succeed");
        assert_eq!(resolved, 1, "the single malformed row must be resolved");

        let malformed_resolved: Option<chrono::DateTime<Utc>> = sqlx::query_scalar(
            "SELECT resolved_at FROM attention_occurrences WHERE id = $1",
        )
        .bind(malformed_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(malformed_resolved.is_some());

        let canonical_row: Option<(Uuid, Option<chrono::DateTime<Utc>>)> = sqlx::query_as(
            "SELECT id, resolved_at FROM attention_occurrences \
             WHERE category = 'builds' AND subject_id = $1 AND source_occurrence_key = $2",
        )
        .bind(&subject_id)
        .bind(&canonical_key)
        .fetch_optional(&pool)
        .await
        .unwrap();
        let (canonical_id, canonical_resolved) =
            canonical_row.expect("a canonical row must have been created from the single malformed row");
        assert!(canonical_resolved.is_none(), "the canonical row must be open");

        let migrated: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM user_attention_dismissals WHERE user_id = $1 AND occurrence_id = $2",
        )
        .bind(user_id)
        .bind(canonical_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(migrated, 1, "dismissal must be migrated to the canonical row");

        let _ = sqlx::query("DELETE FROM attention_occurrences WHERE subject_id = $1")
            .bind(&subject_id)
            .execute(&pool)
            .await;
        let _ = sqlx::query("DELETE FROM build_jobs WHERE id = $1")
            .bind(job_id)
            .execute(&pool)
            .await;
        let _ = sqlx::query("DELETE FROM derivations WHERE id = $1")
            .bind(derivation_id)
            .execute(&pool)
            .await;
        cleanup_user(&pool, user_id).await;
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn test_canonicalize_build_resolved_canonical_plus_open_malformed_reopens_canonical() {
        // Regression test for round 11: a prior reconciliation bug could
        // have incorrectly resolved the canonical `build:<job_id>` row
        // while leaving a malformed duplicate open for the same job. Since
        // a build job can never legitimately recur under one subject_id,
        // this combination proves the canonical row's resolution was a
        // mis-resolve artifact, not a genuine closure -- so it must be
        // reopened rather than left resolved while a malformed row remains
        // the only active representation of an unresolved failure.
        let pool = test_pool().await;
        let job_id = Uuid::new_v4();
        let subject_id = job_id.to_string();
        let canonical_key = format!("build:{subject_id}");

        // Round 12: canonicalizer checks build_jobs.status.
        let short_reopen = Uuid::new_v4().simple().to_string()[..12].to_string();
        let derivation_id: i32 = sqlx::query_scalar(
            "INSERT INTO derivations (derivation_type, derivation_name, status_id, attempt_count) \
             VALUES ('package', $1, 11, 0) RETURNING id",
        )
        .bind(format!("test-canon-reopen-{short_reopen}"))
        .fetch_one(&pool)
        .await
        .unwrap();
        let bj_completed_at = chrono::Utc::now();
        sqlx::query(
            "INSERT INTO build_jobs (id, derivation_id, status, completed_at) \
             VALUES ($1, $2, 'failed', $3)",
        )
        .bind(job_id)
        .bind(derivation_id)
        .bind(bj_completed_at)
        .execute(&pool)
        .await
        .unwrap();
        // Read back completed_at at database precision (microseconds) for
        // exact assertion against the canonical occurrence's opened_at.
        let bj_completed_at: chrono::DateTime<Utc> = sqlx::query_scalar(
            "SELECT completed_at FROM build_jobs WHERE id = $1",
        )
        .bind(job_id)
        .fetch_one(&pool)
        .await
        .unwrap();

        let malformed_key = format!("builds_malformed_test:{subject_id}");

        let canonical_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO attention_occurrences (id, category, subject_type, subject_id, source_occurrence_key, opened_at, last_observed_at, resolved_at) \
             VALUES ($1, 'builds', 'build_job', $2, $3, now() - interval '3 hours', now() - interval '3 hours', now() - interval '2 hours')",
        )
        .bind(canonical_id)
        .bind(&subject_id)
        .bind(&canonical_key)
        .execute(&pool)
        .await
        .unwrap();

        let malformed_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO attention_occurrences (id, category, subject_type, subject_id, source_occurrence_key, opened_at, last_observed_at) \
             VALUES ($1, 'builds', 'build_job', $2, $3, now() - interval '3 hours', now() - interval '3 hours')",
        )
        .bind(malformed_id)
        .bind(&subject_id)
        .bind(&malformed_key)
        .execute(&pool)
        .await
        .unwrap();

        let resolved = dedupe_open_occurrences(&pool)
            .await
            .expect("dedupe should succeed");
        assert_eq!(resolved, 1, "the malformed row must be resolved");

        let canonical_resolved: Option<chrono::DateTime<Utc>> = sqlx::query_scalar(
            "SELECT resolved_at FROM attention_occurrences WHERE id = $1",
        )
        .bind(canonical_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(
            canonical_resolved.is_none(),
            "the canonical row must be reopened, not left resolved"
        );

        // Round 13: the reopened canonical row's opened_at must equal
        // build_jobs.completed_at (not the old 3-hours-ago timestamp).
        let canonical_opened_at: chrono::DateTime<Utc> = sqlx::query_scalar(
            "SELECT opened_at FROM attention_occurrences WHERE id = $1",
        )
        .bind(canonical_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            canonical_opened_at, bj_completed_at,
            "the reopened canonical row's opened_at must equal build_jobs.completed_at"
        );

        let malformed_resolved: Option<chrono::DateTime<Utc>> = sqlx::query_scalar(
            "SELECT resolved_at FROM attention_occurrences WHERE id = $1",
        )
        .bind(malformed_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(malformed_resolved.is_some(), "the malformed row must be resolved");

        let _ = sqlx::query("DELETE FROM attention_occurrences WHERE subject_id = $1")
            .bind(&subject_id)
            .execute(&pool)
            .await;
        let _ = sqlx::query("DELETE FROM build_jobs WHERE id = $1")
            .bind(job_id)
            .execute(&pool)
            .await;
        let _ = sqlx::query("DELETE FROM derivations WHERE id = $1")
            .bind(derivation_id)
            .execute(&pool)
            .await;
    }

    async fn insert_throwaway_flake(pool: &PgPool) -> i32 {
        let short = Uuid::new_v4().simple().to_string()[..12].to_string();
        sqlx::query_scalar::<_, i32>(
            "INSERT INTO flakes (name, repo_url, branch) VALUES ($1, $2, 'main') RETURNING id",
        )
        .bind(format!("att-dedupe-flake-{short}"))
        .bind(format!("https://git.example/att-dedupe-flake-{short}.git"))
        .fetch_one(pool)
        .await
        .expect("failed to insert throwaway test flake")
    }

    async fn insert_throwaway_commit(pool: &PgPool, flake_id: i32) -> i32 {
        let hash = Uuid::new_v4().simple().to_string();
        sqlx::query_scalar::<_, i32>(
            "INSERT INTO commits (flake_id, git_commit_hash, commit_timestamp) \
             VALUES ($1, $2, NOW()) RETURNING id",
        )
        .bind(flake_id)
        .bind(hash)
        .fetch_one(pool)
        .await
        .expect("failed to insert throwaway test commit")
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn test_dedupe_evals_merges_only_exact_timestamp_duplicates() {
        // Two rows for the SAME failure event (identical opened_at, as any
        // bug that computes two different source_occurrence_key values for
        // one evaluation_completed_at would produce, regardless of the
        // specific encoding mismatch involved) are true duplicates and
        // should be merged, including migrating the dismissal between them.
        let pool = test_pool().await;
        let flake_id = insert_throwaway_flake(&pool).await;
        let commit_id = insert_throwaway_commit(&pool, flake_id).await;
        let subject_id = commit_id.to_string();
        let user_id = insert_throwaway_user(&pool).await;

        let t = Utc::now();
        sqlx::query(
            "UPDATE commits SET evaluation_status = 'failed', evaluation_completed_at = $2 WHERE id = $1",
        )
        .bind(commit_id)
        .bind(t)
        .execute(&pool)
        .await
        .unwrap();

        let dup_a = Uuid::new_v4();
        let dup_b = Uuid::new_v4();
        for (id, suffix) in [(dup_a, "a"), (dup_b, "b")] {
            sqlx::query(
                "INSERT INTO attention_occurrences (id, category, subject_type, subject_id, source_occurrence_key, opened_at, last_observed_at) \
                 VALUES ($1, 'evals', 'commit_eval', $2, $3, $4, $4)",
            )
            .bind(id)
            .bind(&subject_id)
            .bind(format!("eval:{commit_id}:{suffix}"))
            .bind(t)
            .execute(&pool)
            .await
            .unwrap();
        }
        sqlx::query(
            "INSERT INTO user_attention_dismissals (user_id, occurrence_id, dismissed_at) VALUES ($1, $2, NOW())",
        )
        .bind(user_id)
        .bind(dup_b)
        .execute(&pool)
        .await
        .unwrap();

        let resolved = dedupe_open_occurrences(&pool)
            .await
            .expect("dedupe should succeed");
        assert_eq!(resolved, 1, "exactly one of the two exact duplicates must be resolved");

        let open_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM attention_occurrences WHERE subject_id = $1 AND resolved_at IS NULL",
        )
        .bind(&subject_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(open_count, 1, "exactly one occurrence must remain open");

        // A dismissal record can be left behind on the now-resolved
        // duplicate too (harmless — resolved rows never contribute to
        // badge counts regardless of dismissal state). What matters is
        // that the SURVIVING open occurrence carries the dismissal.
        let open_and_dismissed: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM user_attention_dismissals uad \
             JOIN attention_occurrences ao ON ao.id = uad.occurrence_id \
             WHERE ao.subject_id = $1 AND ao.resolved_at IS NULL AND uad.user_id = $2",
        )
        .bind(&subject_id)
        .bind(user_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            open_and_dismissed, 1,
            "the dismissal must have migrated onto the surviving (open) duplicate"
        );

        let _ = sqlx::query("DELETE FROM attention_occurrences WHERE subject_id = $1")
            .bind(&subject_id)
            .execute(&pool)
            .await;
        let _ = sqlx::query("DELETE FROM flakes WHERE id = $1")
            .bind(flake_id)
            .execute(&pool)
            .await;
        cleanup_user(&pool, user_id).await;
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn test_dedupe_evals_does_not_transfer_dismissals_across_distinct_episodes() {
        // Regression test for round 8: a naive dedup that groups only by
        // (category, subject_id) could migrate a dismissal from an OLDER,
        // unrelated failure episode onto a NEWER, genuinely-undismissed one
        // — silently hiding a real alert the user has never seen. Distinct
        // `opened_at` values are distinct episodes and must never share a
        // dedup group, so neither row here should be touched at all.
        let pool = test_pool().await;
        let flake_id = insert_throwaway_flake(&pool).await;
        let commit_id = insert_throwaway_commit(&pool, flake_id).await;
        let subject_id = commit_id.to_string();
        let user_id = insert_throwaway_user(&pool).await;

        let t1 = Utc::now() - Duration::hours(2);
        let t2 = Utc::now();

        // The commit is CURRENTLY failed at t2 — a distinct, later episode.
        sqlx::query(
            "UPDATE commits SET evaluation_status = 'failed', evaluation_completed_at = $2 WHERE id = $1",
        )
        .bind(commit_id)
        .bind(t2)
        .execute(&pool)
        .await
        .unwrap();

        // O1: an older episode, left open by a historical lifecycle bug,
        // and already dismissed by the user.
        let o1 = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO attention_occurrences (id, category, subject_type, subject_id, source_occurrence_key, opened_at, last_observed_at) \
             VALUES ($1, 'evals', 'commit_eval', $2, $3, $4, $4)",
        )
        .bind(o1)
        .bind(&subject_id)
        .bind(format!("eval:{commit_id}:o1"))
        .bind(t1)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO user_attention_dismissals (user_id, occurrence_id, dismissed_at) VALUES ($1, $2, NOW())",
        )
        .bind(user_id)
        .bind(o1)
        .execute(&pool)
        .await
        .unwrap();

        // O2: the genuinely new, current failure — never dismissed.
        let o2 = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO attention_occurrences (id, category, subject_type, subject_id, source_occurrence_key, opened_at, last_observed_at) \
             VALUES ($1, 'evals', 'commit_eval', $2, $3, $4, $4)",
        )
        .bind(o2)
        .bind(&subject_id)
        .bind(format!("eval:{commit_id}:o2"))
        .bind(t2)
        .execute(&pool)
        .await
        .unwrap();

        let resolved = dedupe_open_occurrences(&pool)
            .await
            .expect("dedupe should succeed");
        assert_eq!(
            resolved, 0,
            "distinct-timestamp episodes must not be treated as duplicates of each other"
        );

        let o2_dismissed: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM user_attention_dismissals WHERE user_id = $1 AND occurrence_id = $2",
        )
        .bind(user_id)
        .bind(o2)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            o2_dismissed, 0,
            "the current, never-seen failure must NOT inherit the older episode's dismissal"
        );

        let o2_open: Option<DateTime<Utc>> =
            sqlx::query_scalar("SELECT resolved_at FROM attention_occurrences WHERE id = $1")
                .bind(o2)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(o2_open.is_none(), "the current failure's occurrence must remain open");

        let _ = sqlx::query("DELETE FROM attention_occurrences WHERE subject_id = $1")
            .bind(&subject_id)
            .execute(&pool)
            .await;
        let _ = sqlx::query("DELETE FROM flakes WHERE id = $1")
            .bind(flake_id)
            .execute(&pool)
            .await;
        cleanup_user(&pool, user_id).await;
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn test_dedupe_environments_preserves_distinct_system_incidents() {
        // Regression test for round 7: two different systems contributing
        // independent incidents to the SAME environment must not be
        // collapsed into one by dedup — subject_id alone (the environment
        // id) is not a unique incident identity for this category.
        let pool = test_pool().await;
        let env_id = insert_throwaway_environment(&pool, "dedupe-multi").await;
        let system_a = insert_throwaway_system(&pool, env_id).await;
        let system_b = insert_throwaway_system(&pool, env_id).await;

        let occ_a = Uuid::new_v4();
        let occ_b = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO attention_occurrences (id, category, subject_type, subject_id, source_occurrence_key, opened_at, last_observed_at, metadata) \
             VALUES ($1, 'environments', 'environment', $2, $3, now(), now(), $4::jsonb)",
        )
        .bind(occ_a)
        .bind(env_id.to_string())
        .bind(format!("environment:{env_id}:{}", Uuid::new_v4()))
        .bind(serde_json::json!({
            "reason": "critical",
            "underlying_system_id": system_a.to_string(),
        }))
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO attention_occurrences (id, category, subject_type, subject_id, source_occurrence_key, opened_at, last_observed_at, metadata) \
             VALUES ($1, 'environments', 'environment', $2, $3, now(), now(), $4::jsonb)",
        )
        .bind(occ_b)
        .bind(env_id.to_string())
        .bind(format!("environment:{env_id}:{}", Uuid::new_v4()))
        .bind(serde_json::json!({
            "reason": "critical",
            "underlying_system_id": system_b.to_string(),
        }))
        .execute(&pool)
        .await
        .unwrap();

        let resolved = dedupe_open_occurrences(&pool)
            .await
            .expect("dedupe should succeed");
        assert_eq!(
            resolved, 0,
            "two distinct systems' incidents in the same environment must not be treated as duplicates"
        );

        let both_open: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM attention_occurrences \
             WHERE category = 'environments' AND subject_id = $1 AND resolved_at IS NULL",
        )
        .bind(env_id.to_string())
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            both_open, 2,
            "both systems' independent occurrences must remain open"
        );

        let _ = sqlx::query("DELETE FROM attention_occurrences WHERE subject_id = $1")
            .bind(env_id.to_string())
            .execute(&pool)
            .await;
        cleanup_environment(&pool, env_id).await;
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn test_dedupe_environments_resolves_all_duplicates_same_system() {
        // Regression test for round 9+: two rows for the SAME environment
        // and the SAME underlying system, from two distinct system-health
        // episodes (e.g. an old, dismissed critical episode left open by a
        // historical bug, and a later, separate offline episode), must
        // resolve BOTH rows — timestamp-only heuristics (keep-freshest by
        // last_observed_at) cannot distinguish these cases and may preserve
        // a stale dismissed row over a current undismissed one. A system
        // can also move environments and back (A -> B -> A) without its own
        // episode key changing, so underlying_system_occurrence_key cannot
        // be used as a reliable "same episode" distinguisher for
        // environments; grouping is by (environment, underlying_system_id)
        // only, and all rows in the resulting "duplicate" group are
        // resolved (never merge, never migrate dismissal). Environment
        // occurrences are reconstructed by the periodic system reconciler.
        let pool = test_pool().await;
        let env_id = insert_throwaway_environment(&pool, "dedupe-episodes").await;
        let system_id = insert_throwaway_system(&pool, env_id).await;
        let user_id = insert_throwaway_user(&pool).await;

        let old_episode = Uuid::new_v4();
        let new_episode = Uuid::new_v4();
        let occ_old = Uuid::new_v4();
        let occ_new = Uuid::new_v4();

        sqlx::query(
            "INSERT INTO attention_occurrences (id, category, subject_type, subject_id, source_occurrence_key, opened_at, last_observed_at, metadata) \
             VALUES ($1, 'environments', 'environment', $2, $3, now() - interval '1 hour', now() - interval '1 hour', $4::jsonb)",
        )
        .bind(occ_old)
        .bind(env_id.to_string())
        .bind(format!("environment:{env_id}:{old_episode}"))
        .bind(serde_json::json!({
            "reason": "critical",
            "underlying_system_id": system_id.to_string(),
            "underlying_system_occurrence_key": format!("system:{system_id}:critical:{old_episode}"),
        }))
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO user_attention_dismissals (user_id, occurrence_id, dismissed_at) VALUES ($1, $2, NOW())",
        )
        .bind(user_id)
        .bind(occ_old)
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO attention_occurrences (id, category, subject_type, subject_id, source_occurrence_key, opened_at, last_observed_at, metadata) \
             VALUES ($1, 'environments', 'environment', $2, $3, now(), now(), $4::jsonb)",
        )
        .bind(occ_new)
        .bind(env_id.to_string())
        .bind(format!("environment:{env_id}:{new_episode}"))
        .bind(serde_json::json!({
            "reason": "offline",
            "underlying_system_id": system_id.to_string(),
            "underlying_system_occurrence_key": format!("system:{system_id}:offline:{new_episode}"),
        }))
        .execute(&pool)
        .await
        .unwrap();

        let resolved = dedupe_open_occurrences(&pool)
            .await
            .expect("dedupe should succeed");
        assert_eq!(
            resolved, 2,
            "both duplicate episodes must be resolved (not keep-freshest)"
        );

        let old_resolved: Option<DateTime<Utc>> =
            sqlx::query_scalar("SELECT resolved_at FROM attention_occurrences WHERE id = $1")
                .bind(occ_old)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(old_resolved.is_some(), "the older episode must be resolved");

        let new_resolved: Option<DateTime<Utc>> =
            sqlx::query_scalar("SELECT resolved_at FROM attention_occurrences WHERE id = $1")
                .bind(occ_new)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(new_resolved.is_some(), "both episodes are duplicates and must be resolved");

        let new_dismissed: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM user_attention_dismissals WHERE user_id = $1 AND occurrence_id = $2",
        )
        .bind(user_id)
        .bind(occ_new)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            new_dismissed, 0,
            "the new episode must NEVER inherit the old episode's dismissal"
        );

        let _ = sqlx::query("DELETE FROM attention_occurrences WHERE subject_id = $1")
            .bind(env_id.to_string())
            .execute(&pool)
            .await;
        cleanup_environment(&pool, env_id).await;
        cleanup_user(&pool, user_id).await;
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn test_dedupe_environments_null_underlying_system_id_is_not_a_wildcard() {
        // Regression test for round 8: a malformed legacy row with a NULL
        // underlying_system_id must only be treated as a duplicate of
        // OTHER equally-NULL rows, never as a wildcard that matches every
        // occurrence for the environment regardless of their real
        // underlying_system_id.
        let pool = test_pool().await;
        let env_id = insert_throwaway_environment(&pool, "dedupe-null").await;
        let system_id = insert_throwaway_system(&pool, env_id).await;

        let malformed = Uuid::new_v4();
        let valid = Uuid::new_v4();

        // Malformed row: metadata present but missing underlying_system_id
        // entirely (so `metadata->>'underlying_system_id'` is SQL NULL).
        sqlx::query(
            "INSERT INTO attention_occurrences (id, category, subject_type, subject_id, source_occurrence_key, opened_at, last_observed_at, metadata) \
             VALUES ($1, 'environments', 'environment', $2, $3, now() - interval '1 hour', now() - interval '1 hour', $4::jsonb)",
        )
        .bind(malformed)
        .bind(env_id.to_string())
        .bind(format!("environment:{env_id}:{}", Uuid::new_v4()))
        .bind(serde_json::json!({"reason": "critical"}))
        .execute(&pool)
        .await
        .unwrap();

        // Valid row: real underlying_system_id, different from the
        // malformed row's (absent) value.
        sqlx::query(
            "INSERT INTO attention_occurrences (id, category, subject_type, subject_id, source_occurrence_key, opened_at, last_observed_at, metadata) \
             VALUES ($1, 'environments', 'environment', $2, $3, now(), now(), $4::jsonb)",
        )
        .bind(valid)
        .bind(env_id.to_string())
        .bind(format!("environment:{env_id}:{}", Uuid::new_v4()))
        .bind(serde_json::json!({
            "reason": "critical",
            "underlying_system_id": system_id.to_string(),
        }))
        .execute(&pool)
        .await
        .unwrap();

        let resolved = dedupe_open_occurrences(&pool)
            .await
            .expect("dedupe should succeed");
        assert_eq!(
            resolved, 0,
            "a NULL underlying_system_id must not wildcard-match a row with a real one"
        );

        let both_open: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM attention_occurrences \
             WHERE category = 'environments' AND subject_id = $1 AND resolved_at IS NULL",
        )
        .bind(env_id.to_string())
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(both_open, 2, "both rows must remain open and untouched");

        let _ = sqlx::query("DELETE FROM attention_occurrences WHERE subject_id = $1")
            .bind(env_id.to_string())
            .execute(&pool)
            .await;
        cleanup_environment(&pool, env_id).await;
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn test_dedupe_systems_resolves_all_duplicates_without_migrating_dismissal() {
        // Regression test for round 9+: systems are episode-based
        // (transition_by_subject mints a fresh episode UUID on every
        // reason change), so two open rows for one system are NOT provably
        // the same episode -- e.g. a dismissed, unresolved critical episode
        // S1 followed by a genuinely new, later critical episode S2 (a
        // historical bug could leave S1 open across a recovery). Blindly
        // merging and migrating S1's dismissal onto S2 would silently hide
        // the new incident. BOTH rows must be resolved (not keep-freshest),
        // since a historical buggy reconciler could have observed S1 after
        // S2 opened, making the stale S1 appear "freshest". The periodic
        // system reconciler recreates the current occurrence.
        let pool = test_pool().await;
        let env_id = insert_throwaway_environment(&pool, "dedupe-sys").await;
        let system_id = insert_throwaway_system(&pool, env_id).await;
        let user_id = insert_throwaway_user(&pool).await;
        let subject_id = system_id.to_string();

        let s1 = Uuid::new_v4();
        let s2 = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO attention_occurrences (id, category, subject_type, subject_id, source_occurrence_key, opened_at, last_observed_at, metadata) \
             VALUES ($1, 'systems', 'system_health', $2, $3, now() - interval '2 hours', now() - interval '2 hours', $4::jsonb)",
        )
        .bind(s1)
        .bind(&subject_id)
        .bind(format!("system:{subject_id}:critical:{}", Uuid::new_v4()))
        .bind(serde_json::json!({"reason": "critical"}))
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO user_attention_dismissals (user_id, occurrence_id, dismissed_at) VALUES ($1, $2, NOW())",
        )
        .bind(user_id)
        .bind(s1)
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO attention_occurrences (id, category, subject_type, subject_id, source_occurrence_key, opened_at, last_observed_at, metadata) \
             VALUES ($1, 'systems', 'system_health', $2, $3, now(), now(), $4::jsonb)",
        )
        .bind(s2)
        .bind(&subject_id)
        .bind(format!("system:{subject_id}:critical:{}", Uuid::new_v4()))
        .bind(serde_json::json!({"reason": "critical"}))
        .execute(&pool)
        .await
        .unwrap();

        let resolved = dedupe_open_occurrences(&pool)
            .await
            .expect("dedupe should succeed");
        assert_eq!(
            resolved, 2,
            "both duplicate episodes must be resolved (not keep-freshest)"
        );

        let s1_resolved: Option<DateTime<Utc>> =
            sqlx::query_scalar("SELECT resolved_at FROM attention_occurrences WHERE id = $1")
                .bind(s1)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(s1_resolved.is_some(), "the older episode must be resolved");

        let s2_resolved: Option<DateTime<Utc>> =
            sqlx::query_scalar("SELECT resolved_at FROM attention_occurrences WHERE id = $1")
                .bind(s2)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(s2_resolved.is_some(), "both episodes are duplicates and must be resolved");

        let s2_dismissed: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM user_attention_dismissals WHERE user_id = $1 AND occurrence_id = $2",
        )
        .bind(user_id)
        .bind(s2)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            s2_dismissed, 0,
            "the new episode must NEVER inherit the old episode's dismissal"
        );

        let _ = sqlx::query("DELETE FROM attention_occurrences WHERE subject_id = $1")
            .bind(&subject_id)
            .execute(&pool)
            .await;
        cleanup_environment(&pool, env_id).await;
        cleanup_user(&pool, user_id).await;
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn test_dedupe_cves_defaults_to_resolve_all_no_migration_behavior() {
        // Defense-in-depth: `cves` is not explicitly allowlisted as safe to
        // merge (only `evals` and `builds` are; `environments` has its own
        // dedicated resolve-all branch) so it falls through to the
        // generic default branch. Like systems/flakes, cve occurrences are
        // episode-based (a fresh episode UUID per fleet-relevance episode).
        // The default is no longer keep-freshest — all duplicates are
        // resolved, and no dismissal is ever migrated. CVE occurrences
        // are reconstructed by the next CVE vulnerability scan.
        let pool = test_pool().await;
        let user_id = insert_throwaway_user(&pool).await;
        let subject_id = "CVE-2024-TEST-0001".to_string();

        let older = Uuid::new_v4();
        let newer = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO attention_occurrences (id, category, subject_type, subject_id, source_occurrence_key, opened_at, last_observed_at) \
             VALUES ($1, 'cves', 'cve', $2, $3, now() - interval '1 hour', now() - interval '1 hour')",
        )
        .bind(older)
        .bind(&subject_id)
        .bind(format!("cve:{subject_id}:{}", Uuid::new_v4()))
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO user_attention_dismissals (user_id, occurrence_id, dismissed_at) VALUES ($1, $2, NOW())",
        )
        .bind(user_id)
        .bind(older)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO attention_occurrences (id, category, subject_type, subject_id, source_occurrence_key, opened_at, last_observed_at) \
             VALUES ($1, 'cves', 'cve', $2, $3, now(), now())",
        )
        .bind(newer)
        .bind(&subject_id)
        .bind(format!("cve:{subject_id}:{}", Uuid::new_v4()))
        .execute(&pool)
        .await
        .unwrap();

        let resolved = dedupe_open_occurrences(&pool)
            .await
            .expect("dedupe should succeed");
        assert_eq!(
            resolved, 2,
            "both duplicate episodes must be resolved (not keep-freshest)"
        );

        let newer_dismissed: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM user_attention_dismissals WHERE user_id = $1 AND occurrence_id = $2",
        )
        .bind(user_id)
        .bind(newer)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            newer_dismissed, 0,
            "a category not explicitly allowlisted as mergeable must default to never migrating dismissals"
        );

        let _ = sqlx::query("DELETE FROM attention_occurrences WHERE subject_id = $1")
            .bind(&subject_id)
            .execute(&pool)
            .await;
        cleanup_user(&pool, user_id).await;
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn test_dedupe_cves_reconstructs_still_relevant_cve_after_resolving_duplicates() {
        // Regression test for round 11: CVEs have no periodic reconciliation
        // sweep (unlike systems/flakes/environments), so relying on a
        // future vulnerability scan to recreate a resolved-but-still-
        // relevant CVE's occurrence is unbounded. This seeds the FULL
        // chain `view_cve_list_with_metadata` requires (system, nixos
        // derivation, completed scan, package derivation, cve,
        // package_vulnerabilities) so the CVE is genuinely CRITICAL and
        // fleet-relevant (affected_count > 0), then verifies dedupe both
        // resolves the duplicate rows AND immediately reconstructs a fresh
        // occurrence — rather than leaving the badge silently empty.
        let pool = test_pool().await;
        let env_id = insert_throwaway_environment(&pool, "cve-reconstruct").await;
        let system_id = insert_throwaway_system(&pool, env_id).await;
        let short = system_id.simple().to_string()[..12].to_string();
        let hostname: String =
            sqlx::query_scalar("SELECT hostname FROM systems WHERE id = $1")
                .bind(system_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        // cves.id is varchar(20) -- keep well under that limit.
        let cve_id = format!("CVE24-{}", &short[..8]);

        // nixos derivation matching the system's hostname, build-complete.
        let nixos_derivation_id: i32 = sqlx::query_scalar(
            "INSERT INTO derivations (commit_id, derivation_type, derivation_name, status_id, attempt_count) \
             VALUES (NULL, 'nixos', $1, 10, 0) RETURNING id",
        )
        .bind(&hostname)
        .fetch_one(&pool)
        .await
        .unwrap();

        // Completed CVE scan for that derivation.
        let scan_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO cve_scans (id, derivation_id, scanner_name, status, completed_at, \
                                     total_packages, total_vulnerabilities, critical_count) \
             VALUES ($1, $2, 'vulnix', 'completed', NOW(), 1, 1, 1)",
        )
        .bind(scan_id)
        .bind(nixos_derivation_id)
        .execute(&pool)
        .await
        .unwrap();

        // Package derivation, scanned and complete.
        let pkg_name = format!("test-pkg-{short}");
        let pkg_derivation_id: i32 = sqlx::query_scalar(
            "INSERT INTO derivations (commit_id, derivation_type, derivation_name, pname, version, status_id, attempt_count) \
             VALUES (NULL, 'package', $1, 'test-pkg', '1.0.0', 11, 0) RETURNING id",
        )
        .bind(&pkg_name)
        .fetch_one(&pool)
        .await
        .unwrap();

        sqlx::query("INSERT INTO scan_packages (scan_id, derivation_id) VALUES ($1, $2)")
            .bind(scan_id)
            .bind(pkg_derivation_id)
            .execute(&pool)
            .await
            .unwrap();

        // The CVE itself: cvss_v3_score >= 9.0 => severity_from_cvss() = 'CRITICAL'.
        sqlx::query("INSERT INTO cves (id, cvss_v3_score) VALUES ($1, 9.8)")
            .bind(&cve_id)
            .execute(&pool)
            .await
            .unwrap();

        sqlx::query(
            "INSERT INTO package_vulnerabilities (derivation_id, cve_id, is_whitelisted) \
             VALUES ($1, $2, FALSE)",
        )
        .bind(pkg_derivation_id)
        .bind(&cve_id)
        .execute(&pool)
        .await
        .unwrap();

        // Sanity: the view really does consider this CVE critical and
        // fleet-relevant before exercising the repair.
        let (severity, affected_count): (String, i64) = sqlx::query_as(
            "SELECT severity, affected_count FROM view_cve_list_with_metadata WHERE cve_id = $1",
        )
        .bind(&cve_id)
        .fetch_one(&pool)
        .await
        .expect("view_cve_list_with_metadata must return a row for the seeded CVE");
        assert_eq!(severity, "CRITICAL");
        assert!(affected_count > 0);

        // Two duplicate open occurrences for this CVE — the invariant
        // violation dedupe repairs.
        let older = Uuid::new_v4();
        let newer = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO attention_occurrences (id, category, subject_type, subject_id, source_occurrence_key, opened_at, last_observed_at) \
             VALUES ($1, 'cves', 'cve', $2, $3, now() - interval '1 hour', now() - interval '1 hour')",
        )
        .bind(older)
        .bind(&cve_id)
        .bind(format!("cve:{cve_id}:{}", Uuid::new_v4()))
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO attention_occurrences (id, category, subject_type, subject_id, source_occurrence_key, opened_at, last_observed_at) \
             VALUES ($1, 'cves', 'cve', $2, $3, now(), now())",
        )
        .bind(newer)
        .bind(&cve_id)
        .bind(format!("cve:{cve_id}:{}", Uuid::new_v4()))
        .execute(&pool)
        .await
        .unwrap();

        let resolved = dedupe_open_occurrences(&pool)
            .await
            .expect("dedupe should succeed");
        assert_eq!(resolved, 2, "both duplicate CVE occurrences must be resolved");

        let open_rows: Vec<(Uuid, chrono::DateTime<Utc>)> = sqlx::query_as(
            "SELECT id, opened_at FROM attention_occurrences \
             WHERE category = 'cves' AND subject_id = $1 AND resolved_at IS NULL",
        )
        .bind(&cve_id)
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(
            open_rows.len(),
            1,
            "exactly one fresh occurrence must be reconstructed for a still-relevant CVE, \
             rather than leaving the CVE with no open occurrence at all"
        );
        assert!(
            open_rows[0].0 != older && open_rows[0].0 != newer,
            "the reconstructed occurrence must be a NEW row (new episode), not one of the resolved duplicates"
        );

        let _ = sqlx::query("DELETE FROM attention_occurrences WHERE subject_id = $1")
            .bind(&cve_id)
            .execute(&pool)
            .await;
        let _ = sqlx::query("DELETE FROM package_vulnerabilities WHERE cve_id = $1")
            .bind(&cve_id)
            .execute(&pool)
            .await;
        let _ = sqlx::query("DELETE FROM scan_packages WHERE scan_id = $1")
            .bind(scan_id)
            .execute(&pool)
            .await;
        let _ = sqlx::query("DELETE FROM cve_scans WHERE id = $1")
            .bind(scan_id)
            .execute(&pool)
            .await;
        let _ = sqlx::query("DELETE FROM cves WHERE id = $1")
            .bind(&cve_id)
            .execute(&pool)
            .await;
        let _ = sqlx::query("DELETE FROM derivations WHERE id = ANY($1)")
            .bind(vec![nixos_derivation_id, pkg_derivation_id])
            .execute(&pool)
            .await;
        let _ = sqlx::query("DELETE FROM systems WHERE id = $1")
            .bind(system_id)
            .execute(&pool)
            .await;
        cleanup_environment(&pool, env_id).await;
    }

    // ── Round 12: build canonicalization must check authoritative status ──
    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn test_canonicalize_build_does_not_reopen_for_successful_build() {
        // Regression test for round 12: a malformed open occurrence does not
        // prove the build is still failed — the build may have succeeded after
        // the malformed row was created, with the best-effort attention
        // resolution lost to a crash.  The canonicalizer MUST NOT reopen a
        // resolved canonical row (or create a new canonical row) for a
        // non-failed build.
        //
        // Setup:
        //   1. A completed successful build job.
        //   2. A RESOLVED canonical (build:<job_id>) occurrence.
        //   3. An OPEN malformed occurrence (different source_key).
        // After canonicalization, BOTH occurrences must be resolved — no
        // canonical occurrence may be reopened for a non-failed build.
        let pool = test_pool().await;

        // Create a derivation (required FK for build_jobs).
        let short_bca = Uuid::new_v4().simple().to_string()[..12].to_string();
        let derivation_id: i32 = sqlx::query_scalar(
            "INSERT INTO derivations (derivation_type, derivation_name, status_id, attempt_count) \
             VALUES ('package', $1, 11, 0) RETURNING id",
        )
        .bind(format!("test-build-canon-att-{short_bca}"))
        .fetch_one(&pool)
        .await
        .unwrap();

        // Successful build job.
        let job_id = uuid::Uuid::new_v4();
        let subject_id = job_id.to_string();
        let completed_at = chrono::Utc::now();
        sqlx::query(
            "INSERT INTO build_jobs (id, derivation_id, status, completed_at) \
             VALUES ($1, $2, 'success', $3)",
        )
        .bind(job_id)
        .bind(derivation_id)
        .bind(completed_at)
        .execute(&pool)
        .await
        .unwrap();

        // Resolved canonical occurrence.
        let canonical_id = uuid::Uuid::new_v4();
        sqlx::query(
            "INSERT INTO attention_occurrences (id, category, subject_type, subject_id, source_occurrence_key, opened_at, last_observed_at, resolved_at) \
             VALUES ($1, 'builds', 'build_job', $2, $3, now(), now(), now())",
        )
        .bind(canonical_id)
        .bind(&subject_id)
        .bind(format!("build:{job_id}"))
        .execute(&pool)
        .await
        .unwrap();

        // Open malformed occurrence.
        sqlx::query(
            "INSERT INTO attention_occurrences (id, category, subject_type, subject_id, source_occurrence_key, opened_at, last_observed_at) \
             VALUES ($1, 'builds', 'build_job', $2, $3, now(), now())",
        )
        .bind(uuid::Uuid::new_v4())
        .bind(&subject_id)
        .bind(format!("build:{job_id}:malformed"))
        .execute(&pool)
        .await
        .unwrap();

        let resolved = dedupe_open_occurrences(&pool)
            .await
            .expect("dedupe should succeed");

        assert!(resolved > 0, "malformed occurrence must be resolved");

        // Assert: no open occurrences remain (canonical must NOT be reopened).
        let open_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM attention_occurrences \
             WHERE category = 'builds' AND subject_id = $1 AND resolved_at IS NULL",
        )
        .bind(&subject_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            open_count, 0,
            "no open occurrences should remain for a successful build"
        );

        let _ = sqlx::query(
            "DELETE FROM attention_occurrences WHERE category = 'builds' AND subject_id = $1",
        )
        .bind(&subject_id)
        .execute(&pool)
        .await;
        let _ = sqlx::query("DELETE FROM build_jobs WHERE id = $1")
            .bind(job_id)
            .execute(&pool)
            .await;
        let _ = sqlx::query("DELETE FROM derivations WHERE id = $1")
            .bind(derivation_id)
            .execute(&pool)
            .await;
    }
}
