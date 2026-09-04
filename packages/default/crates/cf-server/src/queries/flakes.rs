use crate::api::models::{
    BuildStatus, CommitMetadata, FlakeCommit, FlakeRegistryItem, FlakeTimeline,
};
use crate::config::{FlakeConfig, WatchedFlake};
use crate::models::flakes::{BranchCommitSnapshot, Flake};
use anyhow::Context;
use anyhow::Result;
use sqlx::PgPool;
use uuid::Uuid;

pub async fn insert_flake(
    pool: &PgPool,
    name: &str,
    repo_url: &str,
    branch: &str,
    build_scope: &str,
) -> Result<Flake> {
    let flake = sqlx::query_as::<_, Flake>(
        "
        INSERT INTO flakes (name, repo_url, branch, build_scope)
        VALUES ($1, $2, $3, $4)
        ON CONFLICT (repo_url) DO UPDATE SET 
            name = EXCLUDED.name,
            build_scope = EXCLUDED.build_scope,
            deleted_at = NULL
        RETURNING *
        ",
    )
    .bind(name)
    .bind(repo_url)
    .bind(branch)
    .bind(build_scope)
    .fetch_one(pool)
    .await?;

    Ok(flake)
}

pub async fn get_flake_by_name(pool: &PgPool, name: &str) -> Result<Flake> {
    let commit =
        sqlx::query_as::<_, Flake>("SELECT * FROM flakes WHERE name = $1 AND deleted_at IS NULL")
            .bind(name)
            .fetch_one(pool)
            .await?;

    Ok(commit)
}

pub async fn get_flake_by_id(pool: &PgPool, id: i32) -> Result<Flake> {
    let commit =
        sqlx::query_as::<_, Flake>("SELECT * FROM flakes WHERE id = $1 AND deleted_at IS NULL")
            .bind(id)
            .fetch_one(pool)
            .await?;

    Ok(commit)
}

/// Update only the non-source-identity fields of a flake (`name`, `build_scope`).
///
/// This does NOT touch `repo_url` or `branch` — source-identity changes must go
/// through the locked `reset_flake_source()` path (via `mutate_flake_locked`).
///
/// Takes a transaction reference so the caller can serialise identity changes
/// with the per-flake advisory lock.
pub async fn update_flake_metadata(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    flake_id: i32,
    name: &str,
    build_scope: &str,
) -> Result<Flake> {
    let flake = sqlx::query_as::<_, Flake>(
        r#"
        UPDATE flakes
        SET name = $1,
            build_scope = $2
        WHERE id = $3 AND deleted_at IS NULL
        RETURNING *
        "#,
    )
    .bind(name)
    .bind(build_scope)
    .bind(flake_id)
    .fetch_one(&mut **tx)
    .await
    .context("Failed to update flake metadata")?;

    Ok(flake)
}

pub async fn get_flake_id_by_repo_url(pool: &PgPool, repo_url: &str) -> Result<Option<i32>> {
    let flake_id = sqlx::query_scalar!(
        "SELECT id FROM flakes WHERE repo_url = $1 AND deleted_at IS NULL",
        repo_url
    )
    .fetch_optional(pool)
    .await?;

    Ok(flake_id)
}

pub async fn get_all_flakes_from_db(
    pool: &PgPool,
    config: &FlakeConfig,
) -> Result<Vec<WatchedFlake>> {
    let (flakes, _ids) = get_all_flakes_from_db_with_ids(pool, config).await?;
    Ok(flakes)
}

/// Returns both the `WatchedFlake` list and a parallel vec of database flake IDs.
pub async fn get_all_flakes_from_db_with_ids(
    pool: &PgPool,
    config: &FlakeConfig,
) -> Result<(Vec<WatchedFlake>, Vec<Option<i32>>)> {
    // Use query_as so we don't require an updated sqlx offline cache.
    let rows = sqlx::query_as::<_, (i32, String, String, String)>(
        "SELECT id, name, repo_url, branch FROM flakes WHERE deleted_at IS NULL",
    )
    .fetch_all(pool)
    .await?;

    let mut flakes = Vec::with_capacity(rows.len());
    let mut ids: Vec<Option<i32>> = Vec::with_capacity(rows.len());

    for (id, name, repo_url, branch) in rows {
        let config_flake = config.watched.iter().find(|f| f.repo_url == repo_url);
        flakes.push(WatchedFlake {
            name,
            repo_url,
            branch: Some(branch),
            auto_poll: true,
            initial_commit_depth: config_flake.map(|f| f.initial_commit_depth).unwrap_or(5),
        });
        ids.push(Some(id));
    }

    Ok((flakes, ids))
}

pub async fn find_flake_by_repo_urls(
    pool: &PgPool,
    possible_urls: &[String],
    preferred_url: &str,
) -> Result<Option<crate::models::flakes::Flake>> {
    sqlx::query_as::<_, crate::models::flakes::Flake>(
        r#"
        SELECT id, name, repo_url, branch, build_scope, deleted_at
        FROM flakes 
        WHERE repo_url = ANY($1) AND deleted_at IS NULL
        ORDER BY 
            CASE 
                WHEN repo_url = $2 THEN 1  -- Exact match first
                ELSE 2
            END
        LIMIT 1
        "#,
    )
    .bind(possible_urls)
    .bind(preferred_url)
    .fetch_optional(pool)
    .await
    .context("Failed to find flake by repo URLs")
}

/// Lists flakes and authoritative managed-system counts visible to a caller.
///
/// `visibility_user = None` is reserved for administrators. Other callers see
/// only flakes and active systems joined through their environment memberships.
///
/// # Errors
///
/// Returns an error when PostgreSQL cannot load the visible registry rows.
pub async fn list_flake_registry(
    pool: &PgPool,
    visibility_user: Option<Uuid>,
) -> Result<Vec<FlakeRegistryItem>> {
    // Intermediate row struct matching the query column names.
    // Required because the tuple approach becomes unwieldy with 20+ columns.
    #[derive(sqlx::FromRow)]
    struct FlakeRegistryRow {
        id: i32,
        name: String,
        repo_url: String,
        branch: String,
        build_scope: String,
        system_count: i64,
        sync_status: String,
        last_sync_at: Option<chrono::DateTime<chrono::Utc>>,
        last_sync_error: Option<String>,
        // Enriched fields (TASK-397)
        latest_commit_hash: Option<String>,
        latest_commit_message: Option<String>,
        latest_commit_author: Option<String>,
        latest_commit_timestamp: Option<chrono::DateTime<chrono::Utc>>,
        build_status: Option<String>,
        evaluation_status: Option<String>,
        environments: Vec<String>,
        total_commit_count: i64,
    }

    let rows = sqlx::query_as::<_, FlakeRegistryRow>(
        r#"
        WITH active_flakes AS (
            SELECT *
            FROM flakes
            WHERE deleted_at IS NULL
              AND ($1::uuid IS NULL OR EXISTS (
                  SELECT 1 FROM systems visible_system
                  JOIN user_environment_memberships membership
                    ON membership.environment_id = visible_system.environment_id
                   AND membership.user_id = $1
                  WHERE visible_system.flake_id = flakes.id
                    AND visible_system.is_active = TRUE
              ))
        ),
        system_agg AS (
            SELECT
                s.flake_id,
                COUNT(DISTINCT s.id)::bigint AS system_count,
                COALESCE(
                    array_agg(DISTINCT e.name ORDER BY e.name)
                        FILTER (WHERE s.is_active = TRUE AND e.name IS NOT NULL),
                    ARRAY[]::text[]
                ) AS environments
            FROM systems s
            LEFT JOIN environments e ON e.id = s.environment_id
            WHERE s.flake_id IN (SELECT id FROM active_flakes)
              AND s.is_active = TRUE
              AND ($1::uuid IS NULL OR EXISTS (
                  SELECT 1 FROM user_environment_memberships membership
                  WHERE membership.user_id = $1
                    AND membership.environment_id = s.environment_id
              ))
            GROUP BY s.flake_id
        ),
        snapshot_stats AS (
            SELECT
                fbcs.flake_id,
                (array_agg(fbcs.commit_id ORDER BY fbcs.position))[1] AS head_commit_id,
                COUNT(*)::bigint AS total_count
            FROM flake_branch_commit_snapshot fbcs
            JOIN active_flakes f ON f.id = fbcs.flake_id
            JOIN commits c ON c.id = fbcs.commit_id AND c.source_archived = false
            WHERE f.snapshot_ready_at IS NOT NULL
            GROUP BY fbcs.flake_id
        ),
        fallback_latest AS (
            SELECT DISTINCT ON (c.flake_id)
                c.flake_id,
                c.id AS commit_id,
                COUNT(*) OVER (PARTITION BY c.flake_id)::bigint AS total_count
            FROM commits c
            JOIN active_flakes f ON f.id = c.flake_id
            WHERE f.snapshot_ready_at IS NULL AND c.source_archived = false
            ORDER BY c.flake_id, c.commit_timestamp DESC, c.id DESC
        ),
        effective_commits AS (
            SELECT
                f.id AS flake_id,
                CASE
                    WHEN f.snapshot_ready_at IS NOT NULL THEN ss.head_commit_id
                    ELSE fl.commit_id
                END AS commit_id,
                CASE
                    WHEN f.snapshot_ready_at IS NOT NULL THEN COALESCE(ss.total_count, 0::bigint)
                    ELSE COALESCE(fl.total_count, 0::bigint)
                END AS total_count
            FROM active_flakes f
            LEFT JOIN snapshot_stats ss ON ss.flake_id = f.id
            LEFT JOIN fallback_latest fl ON fl.flake_id = f.id
        ),
        latest_build AS (
            SELECT
                d.commit_id,
                CASE
                    WHEN COUNT(*) FILTER (WHERE bj.status = 'building') > 0 THEN 'building'
                    WHEN COUNT(*) FILTER (WHERE bj.status = 'queued') > 0 THEN 'queued'
                    WHEN COUNT(*) FILTER (WHERE bj.status = 'failed') > 0 THEN 'failed'
                    WHEN COUNT(*) FILTER (WHERE bj.status = 'success') > 0 THEN 'complete'
                    ELSE NULL
                END AS build_status
            FROM build_jobs bj
            JOIN derivations d ON d.id = bj.derivation_id
            WHERE d.commit_id IN (
                SELECT commit_id FROM effective_commits WHERE commit_id IS NOT NULL
            )
              AND ($1::uuid IS NULL OR (
                  (bj.environment_id IS NOT NULL AND EXISTS (
                      SELECT 1 FROM user_environment_memberships membership
                      WHERE membership.user_id = $1
                        AND membership.environment_id = bj.environment_id
                  )) OR (bj.environment_id IS NULL AND EXISTS (
                      SELECT 1 FROM systems visible_system
                      JOIN user_environment_memberships membership
                        ON membership.environment_id = visible_system.environment_id
                      WHERE membership.user_id = $1
                        AND (visible_system.hostname = d.derivation_target
                             OR visible_system.system_configuration_name = d.derivation_target)
                  ))
              ))
            GROUP BY d.commit_id
        )
        SELECT
            f.id,
            f.name,
            f.repo_url,
            f.branch,
            f.build_scope,
            COALESCE(sa.system_count, 0::bigint) AS system_count,
            CASE
                WHEN f.sync_status = 'syncing'
                 AND f.last_sync_at IS NOT NULL
                 AND f.last_sync_at < now() - interval '30 minutes'
                THEN 'error'
                ELSE f.sync_status
            END AS sync_status,
            f.last_sync_at,
            CASE
                WHEN f.sync_status = 'syncing'
                 AND f.last_sync_at IS NOT NULL
                 AND f.last_sync_at < now() - interval '30 minutes'
                THEN COALESCE(f.last_sync_error, 'Sync appears stale — previous sync attempt did not finish')
                ELSE f.last_sync_error
            END AS last_sync_error,
            c.git_commit_hash AS latest_commit_hash,
            c.message AS latest_commit_message,
            c.author AS latest_commit_author,
            c.commit_timestamp AS latest_commit_timestamp,
            lb.build_status,
            c.evaluation_status,
            COALESCE(sa.environments, ARRAY[]::text[]) AS environments,
            ec.total_count AS total_commit_count
        FROM active_flakes f
        JOIN effective_commits ec ON ec.flake_id = f.id
        LEFT JOIN commits c ON c.id = ec.commit_id
        LEFT JOIN latest_build lb ON lb.commit_id = ec.commit_id
        LEFT JOIN system_agg sa ON sa.flake_id = f.id
        ORDER BY lower(f.name) ASC
        "#,
    )
    .bind(visibility_user)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| FlakeRegistryItem {
            id: row.id,
            name: row.name,
            repo_url: row.repo_url,
            branch: row.branch,
            build_scope: row.build_scope,
            system_count: row.system_count,
            sync_status: row.sync_status,
            last_sync_at: row.last_sync_at,
            last_sync_error: row.last_sync_error,
            latest_commit_hash: row.latest_commit_hash,
            latest_commit_message: row.latest_commit_message,
            latest_commit_author: row.latest_commit_author,
            latest_commit_timestamp: row.latest_commit_timestamp,
            build_status: row.build_status,
            evaluation_status: row.evaluation_status,
            environments: row.environments,
            total_commit_count: row.total_commit_count,
        })
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

/// Hard delete a flake by id (no cascade, relies on FK behavior for
/// dependent rows).
///
/// Resolves any open flake attention occurrences under the attention lock
/// before deleting the row, so a deleted flake cannot silently contribute a
/// sidebar badge for the remainder of its 24-hour attention window — the
/// periodic orphan-occurrence sweep in `attention_reconciliation.rs` would
/// eventually catch this on its next 2-minute cycle, but deletion should be
/// immediately authoritative, not merely eventually consistent.
///
/// Acquires the per-flake sync lock BEFORE the attention lock (round 11) —
/// see `soft_delete_flake`'s doc comment for why this fixed order matters
/// across every flake lifecycle operation (this function previously
/// acquired neither lock at all, which is also fixed here).
pub async fn delete_flake_by_id(pool: &PgPool, flake_id: i32) -> Result<u64> {
    use crate::queries::commits::SYNC_LOCK_BASE;

    let mut tx = pool
        .begin()
        .await
        .context("Failed to begin hard-delete transaction")?;

    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(SYNC_LOCK_BASE + i64::from(flake_id))
        .execute(&mut *tx)
        .await
        .context("Failed to acquire per-flake sync lock for hard delete")?;

    let attention_lock_key = format!("attention_occurrence:flakes:{flake_id}");
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(&attention_lock_key)
        .execute(&mut *tx)
        .await
        .context("Failed to acquire flake attention lock for hard delete")?;

    sqlx::query(
        "UPDATE attention_occurrences SET resolved_at = NOW() \
         WHERE category = 'flakes' AND subject_id = $1 AND resolved_at IS NULL",
    )
    .bind(flake_id.to_string())
    .execute(&mut *tx)
    .await
    .context("Failed to resolve flake attention occurrences during hard delete")?;

    // ── Resolve build/eval occurrences before cascade deletes domain rows ──
    let build_ids: Vec<String> = sqlx::query_scalar(
        r#"
        SELECT bj.id::text
        FROM build_jobs bj
        JOIN derivations d ON d.id = bj.derivation_id
        JOIN commits c ON c.id = d.commit_id
        WHERE c.flake_id = $1
        "#,
    )
    .bind(flake_id)
    .fetch_all(&mut *tx)
    .await
    .context("Failed to collect build IDs for attention resolution during hard delete")?;

    let commit_ids: Vec<String> =
        sqlx::query_scalar("SELECT id::text FROM commits WHERE flake_id = $1")
            .bind(flake_id)
            .fetch_all(&mut *tx)
            .await
            .context("Failed to collect commit IDs for attention resolution during hard delete")?;

    sqlx::query(
        r#"
        UPDATE attention_occurrences
        SET resolved_at = statement_timestamp()
        WHERE resolved_at IS NULL
          AND (
              (category = 'builds' AND subject_id = ANY($1::text[]))
              OR
              (category = 'evals' AND subject_id = ANY($2::text[]))
          )
        "#,
    )
    .bind(&build_ids)
    .bind(&commit_ids)
    .execute(&mut *tx)
    .await
    .context("Failed to resolve build/eval attention occurrences during hard delete")?;

    let result = sqlx::query("DELETE FROM flakes WHERE id = $1")
        .bind(flake_id)
        .execute(&mut *tx)
        .await?;

    tx.commit()
        .await
        .context("Failed to commit hard-delete transaction")?;

    Ok(result.rows_affected())
}

/// Atomically resets a flake's source identity and purges unretained history.
///
/// Called when `repo_url` or `branch` changes, OR when reactivating a
/// soft-deleted flake row under a (possibly new) repo_url/branch.  In one
/// transaction:
///
/// 1. Deletes the branch snapshot.
/// 2. Purges caches and history that no retained generation references. A
///    retained derivation, commit, and snapshot remain as immutable generation
///    history but are absent from the new branch snapshot.
/// 3. Updates the flake identity (name, repo_url, branch, build_scope) and
///    clears `deleted_at` (reactivates a soft-deleted row; a no-op for
///    already-active flakes).
/// 4. Sets an empty ready snapshot (`snapshot_ready_at = now()` so readers never
///    see stale commits through fallback ordering).
///
/// Deliberately does NOT filter `WHERE deleted_at IS NULL` — this function is
/// also the reactivation path for soft-deleted flakes, and must be able to
/// find and update those rows.
///
/// The transaction acquires the snapshot-writer lock before the per-flake sync
/// lock and the attention lock. Callers that already hold the per-flake sync
/// lock MUST hold the snapshot-writer lock first.
///
/// # Errors
///
/// Returns an error when PostgreSQL cannot acquire a lock, purge the old source
/// lineage, or persist the replacement source identity.
///
/// Returns the updated `Flake` row.
pub async fn reset_flake_source(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    flake_id: i32,
    name: &str,
    repo_url: &str,
    branch: &str,
    build_scope: &str,
) -> Result<Flake> {
    use crate::queries::commits::SYNC_LOCK_BASE;

    // CONCURRENCY: The global order is snapshot writer, per-flake sync, then
    // attention. Deployment binding/release and source-history preservation
    // therefore cannot commit from incompatible views of the same lineage.
    crate::queries::evaluation_snapshots::lock_snapshot_writer_tx(tx)
        .await
        .context("Failed to acquire snapshot-writer lock for source reset")?;

    // 0. Acquire the per-flake advisory lock that serializes source reset
    //    with sync_flake_recorded's publication transaction.  This prevents
    //    an in-flight old-branch sync from inserting commits or publishing its
    //    snapshot after the reset changes the branch identity.
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(SYNC_LOCK_BASE + i64::from(flake_id))
        .execute(&mut **tx)
        .await
        .context("Failed to acquire per-flake advisory lock for source reset")?;

    // 0.5. Acquire the attention subject lock BEFORE any row mutations below
    //      (round 11): every flake lifecycle operation that touches both the
    //      sync lock and the attention lock (reset, soft delete, cascade
    //      delete, hard delete) must acquire them in the SAME fixed order --
    //      sync lock, then attention lock, then row mutations -- or two such
    //      operations running concurrently could each hold one lock while
    //      waiting for the other, deadlocking (Postgres would abort one).
    //      Acquiring it here, immediately after the sync lock and before any
    //      purge/update below, keeps this function consistent with that
    //      order; the actual resolve of occurrences happens at the end
    //      (step 5) but the lock itself is held from here onward.
    let attention_lock_key = format!("attention_occurrence:flakes:{flake_id}");
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(&attention_lock_key)
        .execute(&mut **tx)
        .await
        .context("Failed to acquire flake attention lock for source reset")?;

    // 1. Clear snapshot
    sqlx::query("DELETE FROM flake_branch_commit_snapshot WHERE flake_id = $1")
        .bind(flake_id)
        .execute(&mut **tx)
        .await
        .context("Failed to clear branch snapshot during source reset")?;

    // 2. Purge commit-scoped caches (same order as purge_flake_commit_history)
    sqlx::query(
        r#"
        DELETE FROM commit_artifacts_cache cac
        USING commits c
        WHERE cac.commit_id = c.id
          AND c.flake_id = $1
        "#,
    )
    .bind(flake_id)
    .execute(&mut **tx)
    .await
    .context("Failed to clear commit artifacts cache during source reset")?;

    sqlx::query(
        r#"
        DELETE FROM commit_metadata_cache cmc
        USING commits c
        WHERE cmc.commit_id = c.id
          AND c.flake_id = $1
        "#,
    )
    .bind(flake_id)
    .execute(&mut **tx)
    .await
    .context("Failed to clear commit metadata cache during source reset")?;

    // ── Resolve build/eval occurrences before domain rows are deleted ─────
    // Round 13: deleting derivations and commits below cascades through
    // build_jobs, leaving orphaned build/eval attention occurrences.  Collect
    // the affected IDs first and resolve them.
    let build_ids: Vec<String> = sqlx::query_scalar(
        r#"
        SELECT bj.id::text
        FROM build_jobs bj
        JOIN derivations d ON d.id = bj.derivation_id
        JOIN commits c ON c.id = d.commit_id
        WHERE c.flake_id = $1
        "#,
    )
    .bind(flake_id)
    .fetch_all(&mut **tx)
    .await
    .context("Failed to collect build IDs for attention resolution during source reset")?;

    let commit_ids: Vec<String> =
        sqlx::query_scalar("SELECT id::text FROM commits WHERE flake_id = $1")
            .bind(flake_id)
            .fetch_all(&mut **tx)
            .await
            .context("Failed to collect commit IDs for attention resolution during source reset")?;

    sqlx::query(
        r#"
        UPDATE attention_occurrences
        SET resolved_at = statement_timestamp()
        WHERE resolved_at IS NULL
          AND (
              (category = 'builds' AND subject_id = ANY($1::text[]))
              OR
              (category = 'evals' AND subject_id = ANY($2::text[]))
          )
        "#,
    )
    .bind(&build_ids)
    .bind(&commit_ids)
    .execute(&mut **tx)
    .await
    .context("Failed to resolve build/eval attention occurrences during source reset")?;

    // RETENTION: Source replacement must remain operational when old commits
    // back retained generations or deployment-bound derivations or artifacts.
    // Delete only derivations that none of these durable identities requires.
    sqlx::query(
        r#"
        DELETE FROM derivations d
        USING commits c
        WHERE d.commit_id = c.id
          AND c.flake_id = $1
           AND NOT EXISTS (
               SELECT 1 FROM evaluation_generation_snapshots retained
               WHERE retained.derivation_id = d.id
           )
           AND NOT EXISTS (
               SELECT 1 FROM pending_system_deployments pending
               WHERE pending.requested_derivation_id = d.id
           )
           AND NOT EXISTS (
               SELECT 1
               FROM pending_system_deployments pending
               JOIN evaluation_snapshots snapshot
                 ON snapshot.id = pending.evaluation_snapshot_id
                AND snapshot.commit_id = pending.requested_commit_id
               WHERE snapshot.commit_id = d.commit_id
                 AND snapshot.configuration_name = d.derivation_name
           )
        "#,
    )
    .bind(flake_id)
    .execute(&mut **tx)
    .await
    .context("Failed to clear derivations during source reset")?;

    // RETENTION: Mark generation-, deployment-, or reservation-bound history
    // as belonging to the replaced source before the flake row receives its
    // new source identity. Source mutation never asks an ON DELETE action to
    // decide whether a deployment identity can be released. The bounded
    // maintenance path owns that decision, including for legacy and path-only
    // rows without exact artifact or derivation bindings. Explicit request
    // reservations remain durable without a time limit. Active revision APIs
    // reject this flag.
    sqlx::query(
        "UPDATE commits c SET source_archived = true WHERE c.flake_id = $1
         AND (
              EXISTS (
                  SELECT 1 FROM evaluation_generation_snapshots retained
                  WHERE retained.commit_id = c.id
               ) OR EXISTS (
                   SELECT 1
                   FROM pending_system_deployments pending
                   WHERE pending.requested_commit_id = c.id
               ) OR EXISTS (
                 SELECT 1
                 FROM pending_system_deployments pending
                 JOIN evaluation_snapshots snapshot
                   ON snapshot.id = pending.evaluation_snapshot_id
                  AND snapshot.commit_id = pending.requested_commit_id
                  WHERE snapshot.commit_id = c.id
             ) OR EXISTS (
                 SELECT 1
                 FROM deployment_request_reservations reservation
                 WHERE reservation.requested_commit_id = c.id
             )
         )",
    )
    .bind(flake_id)
    .execute(&mut **tx)
    .await
    .context("Failed to archive retained commits during source reset")?;

    // Retained commits stay queryable by generation identity but cannot appear
    // as revisions of the new source.
    sqlx::query(
        "DELETE FROM commits c WHERE c.flake_id = $1
          AND NOT EXISTS (
              SELECT 1 FROM evaluation_generation_snapshots retained
              WHERE retained.commit_id = c.id
           ) AND NOT EXISTS (
               SELECT 1
               FROM pending_system_deployments pending
               WHERE pending.requested_commit_id = c.id
           ) AND NOT EXISTS (
             SELECT 1
             FROM pending_system_deployments pending
             JOIN evaluation_snapshots snapshot
               ON snapshot.id = pending.evaluation_snapshot_id
              AND snapshot.commit_id = pending.requested_commit_id
              WHERE snapshot.commit_id = c.id
         ) AND NOT EXISTS (
             SELECT 1
             FROM deployment_request_reservations reservation
             WHERE reservation.requested_commit_id = c.id
         )",
    )
    .bind(flake_id)
    .execute(&mut **tx)
    .await
    .context("Failed to clear commits during source reset")?;

    // 3+4. Update flake identity, reactivate (clear deleted_at), and reset all
    //      sync/snapshot state atomically.  Combining these into one UPDATE
    //      ensures the RETURNING row reflects the final state, not an
    //      intermediate one.  No `deleted_at IS NULL` filter — this is also
    //      the reactivation path for soft-deleted rows.
    let flake = sqlx::query_as::<_, Flake>(
        r#"
        UPDATE flakes
        SET name             = $1,
            repo_url         = $2,
            branch           = $3,
            build_scope      = $4,
            deleted_at       = NULL,
            snapshot_ready_at = now(),
            sync_attempt_id  = NULL,
            sync_status      = 'unknown',
            last_sync_at     = NULL,
            last_sync_error  = NULL,
            last_synced_at   = NULL
        WHERE id = $5
        RETURNING *
        "#,
    )
    .bind(name)
    .bind(repo_url)
    .bind(branch)
    .bind(build_scope)
    .bind(flake_id)
    .fetch_one(&mut **tx)
    .await
    .context("Failed to update flake identity and reset state during source reset")?;

    // 5. Create a clean attention-episode boundary.  The source identity
    //    (repo_url/branch) has been replaced — an incident from the old
    //    source must not be silently inherited by the new one.
    //
    //    Resolve all open flake attention occurrences for this flake — the
    //    attention lock was already acquired in step 0.5, before any of the
    //    mutations above, so this resolve is still serialized with any
    //    concurrent reconciler (which holds the same lock key). The
    //    `last_synced_at` column was already cleared above, so after this
    //    resolve, any future sync failure on the new source identity
    //    starts with a clean lineage slate — a success on the old source
    //    cannot make a new-source failure appear continuous with the old
    //    source's incident.
    sqlx::query(
        "UPDATE attention_occurrences SET resolved_at = NOW() \
         WHERE category = 'flakes' AND subject_id = $1 AND resolved_at IS NULL",
    )
    .bind(flake_id.to_string())
    .execute(&mut **tx)
    .await
    .context("Failed to resolve flake attention occurrences during source reset")?;

    Ok(flake)
}

/// Mutates an existing flake's identity or metadata under ordered advisory locks.
///
/// The function acquires the snapshot-writer lock before the per-flake lock and
/// re-reads the current row inside the locks before it decides what to write.
/// This is the single linearization point for both the create and
/// update HTTP handlers: any write that could touch `repo_url` or `branch`
/// must go through this function (or `create_or_mutate_flake` below) so a
/// concurrent identity change is never silently clobbered or silently
/// inherited.
///
/// Returns `Ok(None)` if the row no longer exists (e.g. hard-deleted by a
/// concurrent request after the caller resolved `flake_id` but before this
/// function acquired the lock). Callers decide how to interpret that: the
/// update handler maps it to 404, `create_or_mutate_flake` retries.
///
/// If the locked row's `repo_url`, `branch`, or deletion state differs from
/// the requested identity, the source is reset via `reset_flake_source`
/// (purges history, clears the snapshot, invalidates in-flight syncs).
/// Otherwise only non-source metadata (`name`, `build_scope`) is updated.
///
/// # Errors
///
/// Returns an error when PostgreSQL cannot acquire a lock, read the current
/// identity, reset the source, update metadata, or commit the transaction.
pub async fn mutate_flake_locked(
    pool: &PgPool,
    flake_id: i32,
    name: &str,
    repo_url: &str,
    branch: &str,
    build_scope: &str,
) -> Result<Option<Flake>> {
    use crate::queries::commits::SYNC_LOCK_BASE;

    let mut tx = pool
        .begin()
        .await
        .context("Failed to begin flake mutation tx")?;

    crate::queries::evaluation_snapshots::lock_snapshot_writer_tx(&mut tx)
        .await
        .context("Failed to acquire snapshot-writer lock for flake mutation")?;

    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(SYNC_LOCK_BASE + i64::from(flake_id))
        .execute(&mut *tx)
        .await
        .context("Failed to acquire advisory lock for flake mutation")?;

    let current = sqlx::query_as::<_, (String, String, bool)>(
        "SELECT repo_url, branch, (deleted_at IS NOT NULL) AS is_deleted \
         FROM flakes WHERE id = $1",
    )
    .bind(flake_id)
    .fetch_optional(&mut *tx)
    .await
    .context("Failed to re-read flake identity under lock")?;

    let Some((cur_repo_url, cur_branch, is_deleted)) = current else {
        let _ = tx.rollback().await;
        return Ok(None);
    };

    let flake = if cur_repo_url != repo_url || cur_branch != branch || is_deleted {
        reset_flake_source(&mut tx, flake_id, name, repo_url, branch, build_scope).await?
    } else {
        update_flake_metadata(&mut tx, flake_id, name, build_scope).await?
    };

    tx.commit()
        .await
        .context("Failed to commit flake mutation")?;

    Ok(Some(flake))
}

/// Outcome of `mutate_conflicting_flake_locked`.
enum ConflictMutation {
    /// The row still owned `expected_repo_url` when the lock was acquired,
    /// and has now been mutated (reset or metadata-updated) accordingly.
    Mutated(Flake),
    /// The row no longer owns `expected_repo_url` — a concurrent request
    /// already moved it to a different `repo_url` between the caller's
    /// unlocked conflict lookup and this function acquiring the lock (or
    /// the row was hard-deleted). The caller must NOT treat this as an
    /// intentional identity change; it must retry conflict resolution from
    /// scratch instead of forcibly reclaiming the row.
    Stale,
}

/// Mutate a flake row that the caller resolved via a create-path
/// `INSERT ... ON CONFLICT` lookup (i.e. the id was inferred indirectly from
/// `repo_url`, not supplied directly by the caller as in `mutate_flake_locked`).
///
/// Acquires the per-flake advisory lock and re-reads the row's CURRENT
/// `repo_url` before doing anything. If it no longer equals
/// `expected_repo_url`, the conflict lookup that produced `flake_id` is
/// stale — some other request already changed this row's identity between
/// our unlocked lookup and acquiring the lock — so this returns
/// `ConflictMutation::Stale` without writing anything. Forcibly resetting
/// the row back to `expected_repo_url` in that situation would silently
/// undo the concurrent request's completed, committed work.
///
/// When the row's `repo_url` still matches, this behaves like
/// `mutate_flake_locked`: reset the source if `branch`/deletion state
/// differs from the request, otherwise update only non-source metadata.
async fn mutate_conflicting_flake_locked(
    pool: &PgPool,
    flake_id: i32,
    expected_repo_url: &str,
    name: &str,
    branch: &str,
    build_scope: &str,
) -> Result<ConflictMutation> {
    use crate::queries::commits::SYNC_LOCK_BASE;

    let mut tx = pool
        .begin()
        .await
        .context("Failed to begin flake mutation tx")?;

    crate::queries::evaluation_snapshots::lock_snapshot_writer_tx(&mut tx)
        .await
        .context("Failed to acquire snapshot-writer lock for flake mutation")?;

    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(SYNC_LOCK_BASE + i64::from(flake_id))
        .execute(&mut *tx)
        .await
        .context("Failed to acquire advisory lock for flake mutation")?;

    let current = sqlx::query_as::<_, (String, String, bool)>(
        "SELECT repo_url, branch, (deleted_at IS NOT NULL) AS is_deleted \
         FROM flakes WHERE id = $1",
    )
    .bind(flake_id)
    .fetch_optional(&mut *tx)
    .await
    .context("Failed to re-read flake identity under lock")?;

    let Some((cur_repo_url, cur_branch, is_deleted)) = current else {
        let _ = tx.rollback().await;
        return Ok(ConflictMutation::Stale);
    };

    if cur_repo_url != expected_repo_url {
        // The row we resolved via the unlocked conflict lookup no longer
        // owns expected_repo_url. Do NOT reset it back — that would
        // silently clobber whatever concurrent request already moved it
        // (see create_or_mutate_flake's doc comment for the exact race).
        let _ = tx.rollback().await;
        return Ok(ConflictMutation::Stale);
    }

    let flake = if cur_branch != branch || is_deleted {
        reset_flake_source(
            &mut tx,
            flake_id,
            name,
            expected_repo_url,
            branch,
            build_scope,
        )
        .await?
    } else {
        update_flake_metadata(&mut tx, flake_id, name, build_scope).await?
    };

    tx.commit()
        .await
        .context("Failed to commit flake mutation")?;

    Ok(ConflictMutation::Mutated(flake))
}

/// Atomically create a flake for `repo_url`, or — if a row already exists for
/// it (active or soft-deleted) — mutate that row under the per-flake
/// advisory lock instead of silently upserting a stale branch.
///
/// The initial insert uses `ON CONFLICT DO NOTHING`, so a genuine conflict
/// never overwrites another row's `branch`/`repo_url`. On conflict, the
/// existing row's id is resolved (via an unlocked `SELECT`) and the
/// mutation is delegated to `mutate_conflicting_flake_locked`, which
/// re-reads `repo_url` under the lock and refuses to touch the row if it no
/// longer matches — that id resolution is inherently racy against a
/// concurrent request that moves the row to a different `repo_url` between
/// our unlocked lookup and acquiring the lock. In that case the row is left
/// untouched and this function retries the whole insert-or-resolve loop,
/// which correctly re-evaluates whether `repo_url` is now free (insert
/// succeeds) or owned by a different row (resolve+lock that one instead).
///
/// This is unlike `mutate_flake_locked`, used by the update-by-id handler:
/// there the caller supplies `flake_id` directly (from the URL path), so a
/// changed identity under the lock is an intentional correction to make,
/// not a stale lookup to discard.
///
/// Bounded retry (5 attempts) handles both the hard-delete race and the
/// stale-conflict race described above.
pub async fn create_or_mutate_flake(
    pool: &PgPool,
    name: &str,
    repo_url: &str,
    branch: &str,
    build_scope: &str,
) -> Result<Flake> {
    for _ in 0..5 {
        let inserted = sqlx::query_as::<_, Flake>(
            "INSERT INTO flakes (name, repo_url, branch, build_scope) \
             VALUES ($1, $2, $3, $4) \
             ON CONFLICT (repo_url) DO NOTHING \
             RETURNING *",
        )
        .bind(name)
        .bind(repo_url)
        .bind(branch)
        .bind(build_scope)
        .fetch_optional(pool)
        .await
        .context("Failed to insert flake")?;

        if let Some(flake) = inserted {
            return Ok(flake);
        }

        // Conflict: an existing row (active or soft-deleted) owns this
        // repo_url. Resolve its id and mutate it under the per-flake lock.
        let existing_id = sqlx::query_scalar::<_, i32>("SELECT id FROM flakes WHERE repo_url = $1")
            .bind(repo_url)
            .fetch_optional(pool)
            .await
            .context("Failed to look up existing flake after insert conflict")?;

        let Some(existing_id) = existing_id else {
            // Row vanished between the conflict and this lookup — retry.
            continue;
        };

        match mutate_conflicting_flake_locked(
            pool,
            existing_id,
            repo_url,
            name,
            branch,
            build_scope,
        )
        .await?
        {
            ConflictMutation::Mutated(flake) => return Ok(flake),
            // The resolved row no longer owns repo_url (or vanished) —
            // retry from the top: repo_url may now be free (insert
            // succeeds) or owned by a different, current row.
            ConflictMutation::Stale => continue,
        }
    }

    anyhow::bail!(
        "Failed to create or update flake for repo_url {repo_url} after repeated conflicts"
    )
}

/// Purge a flake's commit history and reset its sync/snapshot state when an
/// operator explicitly accepts a detected history rewrite.
///
/// Acquires the SAME per-flake advisory lock (`SYNC_LOCK_BASE + flake_id`)
/// used by `sync_flake_recorded` and `reset_flake_source`. This prevents a
/// race where an in-flight sync — started before the rewrite was accepted —
/// inserts commits or publishes a snapshot from the old (divergent) branch
/// state after this purge completes, since that sync's mutation transaction
/// cannot run until this transaction's lock is released.
///
/// In one transaction:
/// 1. Acquires the per-flake advisory lock.
/// 2. Purges commit-scoped caches, derivations, and commits (same order as
///    the identity-reset path).
/// 3. Clears the branch snapshot.
/// 4. Invalidates any in-flight `sync_attempt_id` so a stale sync started
///    before the lock cannot publish under the old `sync_attempt_id` match.
/// 5. Resets `snapshot_ready_at` to `NULL` (not ready) — the caller is
///    expected to immediately re-sync, which will populate a fresh snapshot.
///
/// Returns the number of purged commit rows.
pub async fn accept_history_rewrite_reset(pool: &PgPool, flake_id: i32) -> Result<u64> {
    use crate::queries::commits::SYNC_LOCK_BASE;

    let mut tx = pool.begin().await?;

    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(SYNC_LOCK_BASE + i64::from(flake_id))
        .execute(&mut *tx)
        .await
        .context("Failed to acquire per-flake advisory lock for rewrite acceptance")?;

    // Clear commit-scoped caches first for deterministic cleanup.
    sqlx::query(
        r#"
        DELETE FROM commit_artifacts_cache cac
        USING commits c
        WHERE cac.commit_id = c.id
          AND c.flake_id = $1
        "#,
    )
    .bind(flake_id)
    .execute(&mut *tx)
    .await
    .context("Failed to clear commit artifacts cache during rewrite acceptance")?;

    sqlx::query(
        r#"
        DELETE FROM commit_metadata_cache cmc
        USING commits c
        WHERE cmc.commit_id = c.id
          AND c.flake_id = $1
        "#,
    )
    .bind(flake_id)
    .execute(&mut *tx)
    .await
    .context("Failed to clear commit metadata cache during rewrite acceptance")?;

    // ── Resolve build/eval occurrences before domain rows are deleted ─────
    // Round 13: same rationale as reset_flake_source.
    let build_ids: Vec<String> = sqlx::query_scalar(
        r#"
        SELECT bj.id::text
        FROM build_jobs bj
        JOIN derivations d ON d.id = bj.derivation_id
        JOIN commits c ON c.id = d.commit_id
        WHERE c.flake_id = $1
        "#,
    )
    .bind(flake_id)
    .fetch_all(&mut *tx)
    .await
    .context("Failed to collect build IDs for attention resolution during rewrite acceptance")?;

    let commit_ids: Vec<String> =
        sqlx::query_scalar("SELECT id::text FROM commits WHERE flake_id = $1")
            .bind(flake_id)
            .fetch_all(&mut *tx)
            .await
            .context(
                "Failed to collect commit IDs for attention resolution during rewrite acceptance",
            )?;

    sqlx::query(
        r#"
        UPDATE attention_occurrences
        SET resolved_at = statement_timestamp()
        WHERE resolved_at IS NULL
          AND (
              (category = 'builds' AND subject_id = ANY($1::text[]))
              OR
              (category = 'evals' AND subject_id = ANY($2::text[]))
          )
        "#,
    )
    .bind(&build_ids)
    .bind(&commit_ids)
    .execute(&mut *tx)
    .await
    .context("Failed to resolve build/eval attention occurrences during rewrite acceptance")?;

    // RETENTION: A retained generation or deployment request owns its exact
    // derivation. Preserve that row during rewrite acceptance so restrictive
    // foreign keys do not abort recovery and each request remains attributable.
    sqlx::query(
        r#"
        DELETE FROM derivations d
        USING commits c
        WHERE d.commit_id = c.id
          AND c.flake_id = $1
          AND NOT EXISTS (
              SELECT 1 FROM evaluation_generation_snapshots retained
              WHERE retained.derivation_id = d.id
          )
          AND NOT EXISTS (
              SELECT 1 FROM pending_system_deployments pending
              WHERE pending.requested_derivation_id = d.id
          )
        "#,
    )
    .bind(flake_id)
    .execute(&mut *tx)
    .await
    .context("Failed to clear derivations during rewrite acceptance")?;

    // SECURITY: Every commit that survives the purge belongs to the replaced
    // lineage. Archive the complete lineage before deletion so a snapshot or
    // another restrictive reference cannot leave an old SHA authorized by an
    // active-revision API. A later sync explicitly reactivates SHAs that Git
    // proves belong to the rewritten branch.
    sqlx::query("UPDATE commits SET source_archived = true WHERE flake_id = $1")
        .bind(flake_id)
        .execute(&mut *tx)
        .await
        .context("Failed to archive old-lineage commits during rewrite acceptance")?;

    let deleted_commits = sqlx::query(
        r#"
        DELETE FROM commits c
        WHERE c.flake_id = $1
          AND NOT EXISTS (
              SELECT 1 FROM evaluation_snapshots snapshot
              WHERE snapshot.commit_id = c.id
          )
          AND NOT EXISTS (
              SELECT 1 FROM flake_output_snapshots snapshot
              WHERE snapshot.commit_id = c.id
          )
           AND NOT EXISTS (
               SELECT 1 FROM pending_system_deployments pending
               WHERE pending.requested_commit_id = c.id
           )
           AND NOT EXISTS (
               SELECT 1 FROM deployment_request_reservations reservation
               WHERE reservation.requested_commit_id = c.id
           )
        "#,
    )
    .bind(flake_id)
    .execute(&mut *tx)
    .await
    .context("Failed to clear commits during rewrite acceptance")?
    .rows_affected();

    // Clear the branch snapshot and invalidate any in-flight sync attempt.
    sqlx::query("DELETE FROM flake_branch_commit_snapshot WHERE flake_id = $1")
        .bind(flake_id)
        .execute(&mut *tx)
        .await
        .context("Failed to clear branch snapshot during rewrite acceptance")?;

    sqlx::query(
        "UPDATE flakes \
         SET snapshot_ready_at = NULL, sync_attempt_id = NULL \
         WHERE id = $1",
    )
    .bind(flake_id)
    .execute(&mut *tx)
    .await
    .context("Failed to reset snapshot/sync state during rewrite acceptance")?;

    tx.commit().await?;
    Ok(deleted_commits)
}

/// Soft delete a flake by setting deleted_at timestamp.
/// The flake will be excluded from normal queries but retained for audit.
///
/// Resolves any open flake attention occurrences under the attention lock,
/// so a deleted flake cannot silently contribute a sidebar badge for the
/// remainder of its 24-hour attention window.
///
/// Acquires the per-flake sync lock BEFORE the attention lock (round 11):
/// every flake lifecycle operation that touches both locks must use this
/// same fixed order (sync lock, then attention lock, then row mutations),
/// matching `reset_flake_source`/`cascade_delete_flake`/`delete_flake_by_id`
/// — otherwise two such operations racing on the same flake could each hold
/// one lock while waiting for the other, deadlocking.
pub async fn soft_delete_flake(pool: &PgPool, flake_id: i32) -> Result<u64> {
    use crate::queries::commits::SYNC_LOCK_BASE;

    let mut tx = pool
        .begin()
        .await
        .context("Failed to begin soft-delete transaction")?;

    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(SYNC_LOCK_BASE + i64::from(flake_id))
        .execute(&mut *tx)
        .await
        .context("Failed to acquire per-flake sync lock for soft delete")?;

    // Acquire the flake attention lock so this resolve is serialized with
    // any concurrent reconciler (which holds the same lock key).
    let attention_lock_key = format!("attention_occurrence:flakes:{flake_id}");
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(&attention_lock_key)
        .execute(&mut *tx)
        .await
        .context("Failed to acquire flake attention lock for soft delete")?;

    // Resolve all open occurrences first, then soft-delete.
    sqlx::query(
        "UPDATE attention_occurrences SET resolved_at = NOW() \
         WHERE category = 'flakes' AND subject_id = $1 AND resolved_at IS NULL",
    )
    .bind(flake_id.to_string())
    .execute(&mut *tx)
    .await
    .context("Failed to resolve flake attention occurrences during soft delete")?;

    let result =
        sqlx::query("UPDATE flakes SET deleted_at = NOW() WHERE id = $1 AND deleted_at IS NULL")
            .bind(flake_id)
            .execute(&mut *tx)
            .await?;

    tx.commit()
        .await
        .context("Failed to commit soft-delete transaction")?;

    Ok(result.rows_affected())
}

/// Check if flake has active dependencies (pending/in-progress evaluations, builds, or deployments).
/// Returns count of blocking dependencies.
pub async fn check_flake_dependencies(pool: &PgPool, flake_id: i32) -> Result<i64> {
    // Check if any active systems are using this flake
    //
    // NOTE: The 'evaluations' and 'build_queue' tables are planned features
    // but not yet implemented. When they are added, expand this check to include:
    // - Active evaluations (evaluations.status IN ('pending', 'in_progress'))
    // - Active builds (build_queue.status IN ('pending', 'in_progress'))
    //
    // For now, we only check for systems using the flake, which is the most
    // critical dependency that would break if we deleted the flake.
    let count = sqlx::query_scalar::<_, i64>(
        r#"
        SELECT COUNT(*)::bigint
        FROM systems
        WHERE flake_id = $1
          AND is_active = true
        "#,
    )
    .bind(flake_id)
    .fetch_one(pool)
    .await?;

    Ok(count)
}

/// Cascade delete a flake and all related data (evaluations, builds, deployments).
/// This is a hard delete that permanently removes all traces.
/// MUST be run in a transaction for safety - pass a transaction reference.
///
/// Resolves any open flake attention occurrences under the attention lock,
/// so a deleted flake cannot silently contribute a sidebar badge for the
/// remainder of its 24-hour attention window.
///
/// Acquires the per-flake sync lock BEFORE the attention lock (round 11) —
/// see `soft_delete_flake`'s doc comment for why this fixed order matters
/// across every flake lifecycle operation.
pub async fn cascade_delete_flake(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    flake_id: i32,
) -> Result<u64> {
    use crate::queries::commits::SYNC_LOCK_BASE;

    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(SYNC_LOCK_BASE + i64::from(flake_id))
        .execute(&mut **tx)
        .await
        .context("Failed to acquire per-flake sync lock for cascade delete")?;

    // Acquire the flake attention lock so this resolve is serialized with
    // any concurrent reconciler.
    let flake_attention_lock = format!("attention_occurrence:flakes:{flake_id}");
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(&flake_attention_lock)
        .execute(&mut **tx)
        .await
        .context("Failed to acquire flake attention lock for cascade delete")?;

    // Resolve all open occurrences for this flake before deleting the row.
    sqlx::query(
        "UPDATE attention_occurrences SET resolved_at = NOW() \
         WHERE category = 'flakes' AND subject_id = $1 AND resolved_at IS NULL",
    )
    .bind(flake_id.to_string())
    .execute(&mut **tx)
    .await
    .context("Failed to resolve flake attention occurrences during cascade delete")?;

    // ── Resolve derived system and environment occurrences ────────────────
    // Round 12: cascade deletion previously only resolved the flake's own
    // occurrences. Systems belonging to this flake were deleted without
    // resolving their `systems` or derived `environments` occurrences,
    // leaving orphaned sidebar badges until the periodic stale-occurrence
    // sweep ran (up to 2 minutes later).
    //
    // Now: read the system IDs, sort them deterministically to prevent
    // deadlocks, acquire each system's attention lock, and resolve both
    // `systems` and `environments` occurrences for them in the same
    // transaction.
    let system_ids: Vec<uuid::Uuid> =
        sqlx::query_scalar("SELECT id FROM systems WHERE flake_id = $1 ORDER BY id ASC")
            .bind(flake_id)
            .fetch_all(&mut **tx)
            .await
            .context("Failed to read associated system IDs for cascade delete")?;

    for system_id in &system_ids {
        let sys_lock = format!("attention_occurrence:systems:{system_id}");
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
            .bind(&sys_lock)
            .execute(&mut **tx)
            .await
            .context("Failed to acquire system attention lock during cascade delete")?;
    }

    if !system_ids.is_empty() {
        let system_id_strs: Vec<String> = system_ids.iter().map(|id| id.to_string()).collect();
        // Resolve open systems occurrences for the deleted systems.
        sqlx::query(
            "UPDATE attention_occurrences SET resolved_at = NOW() \
             WHERE category = 'systems' AND subject_id = ANY($1) AND resolved_at IS NULL",
        )
        .bind(&system_id_strs)
        .execute(&mut **tx)
        .await
        .context("Failed to resolve system attention occurrences during cascade delete")?;

        // Resolve open environment occurrences derived from any of these systems.
        sqlx::query(
            r#"
            UPDATE attention_occurrences
            SET resolved_at = NOW()
            WHERE category = 'environments'
              AND resolved_at IS NULL
              AND metadata->>'underlying_system_id' = ANY($1)
            "#,
        )
        .bind(&system_id_strs)
        .execute(&mut **tx)
        .await
        .context("Failed to resolve environment attention occurrences during cascade delete")?;
    }

    // ── Resolve build/eval occurrences before cascade deletes domain rows ──
    // Round 13: same rationale as reset_flake_source.
    let build_ids: Vec<String> = sqlx::query_scalar(
        r#"
        SELECT bj.id::text
        FROM build_jobs bj
        JOIN derivations d ON d.id = bj.derivation_id
        JOIN commits c ON c.id = d.commit_id
        WHERE c.flake_id = $1
        "#,
    )
    .bind(flake_id)
    .fetch_all(&mut **tx)
    .await
    .context("Failed to collect build IDs for attention resolution during cascade delete")?;

    let commit_ids: Vec<String> =
        sqlx::query_scalar("SELECT id::text FROM commits WHERE flake_id = $1")
            .bind(flake_id)
            .fetch_all(&mut **tx)
            .await
            .context(
                "Failed to collect commit IDs for attention resolution during cascade delete",
            )?;

    sqlx::query(
        r#"
        UPDATE attention_occurrences
        SET resolved_at = statement_timestamp()
        WHERE resolved_at IS NULL
          AND (
              (category = 'builds' AND subject_id = ANY($1::text[]))
              OR
              (category = 'evals' AND subject_id = ANY($2::text[]))
          )
        "#,
    )
    .bind(&build_ids)
    .bind(&commit_ids)
    .execute(&mut **tx)
    .await
    .context("Failed to resolve build/eval attention occurrences during cascade delete")?;

    // Note: ON DELETE CASCADE on commits FK will handle most cleanup
    // But we explicitly delete systems first to be safe
    sqlx::query("DELETE FROM systems WHERE flake_id = $1")
        .bind(flake_id)
        .execute(&mut **tx)
        .await?;

    // Delete the flake (commits, evaluations, builds cascade automatically)
    let result = sqlx::query("DELETE FROM flakes WHERE id = $1")
        .bind(flake_id)
        .execute(&mut **tx)
        .await?;

    Ok(result.rows_affected())
}

/// Fetch flake timelines for dashboard view (CF system deployment counts).
///
/// Returns up to `max_commits_per_flake` most recent commits for each flake,
/// showing count of Crystal Forge systems deployed at each commit.
pub async fn fetch_dashboard_flake_timelines(
    pool: &PgPool,
    max_commits_per_flake: i64,
    flake_ids: Option<&[i32]>,
    user_id: Option<uuid::Uuid>,
) -> Result<Vec<FlakeTimeline>> {
    let flake_filter: Option<Vec<i32>> = flake_ids.map(|ids| ids.to_vec());
    #[derive(sqlx::FromRow)]
    struct Row {
        flake_id: i32,
        flake_name: String,
        repo_url: String,
        id: Option<i32>,
        hash: Option<String>,
        message: Option<String>,
        author: Option<String>,
        committed_at: Option<chrono::DateTime<chrono::Utc>>,
        system_count: i64,
        systems: Vec<String>,
        commits_behind: Option<i64>,
        build_status: Option<String>,
        evaluation_status: Option<String>,
        evaluation_error_message: Option<String>,
    }

    let rows = sqlx::query_as::<_, Row>(
        r#"WITH visible_flakes AS (
          SELECT f.id, f.name, f.repo_url, f.snapshot_ready_at
          FROM flakes f
          WHERE f.deleted_at IS NULL AND ($1::int[] IS NULL OR f.id = ANY($1))
            AND ($3::uuid IS NULL OR EXISTS (
              SELECT 1 FROM systems s JOIN user_environment_memberships uem ON uem.environment_id = s.environment_id
              WHERE s.flake_id = f.id AND uem.user_id = $3
            ))
        ), ranked AS (
          SELECT vf.id AS flake_id, vf.name AS flake_name, vf.repo_url, c.id AS commit_id,
                 CASE WHEN vf.snapshot_ready_at IS NOT NULL THEN fbcs.position::bigint
                      ELSE ROW_NUMBER() OVER (PARTITION BY c.flake_id ORDER BY c.commit_timestamp DESC, c.id DESC) - 1 END AS commits_behind,
                 ROW_NUMBER() OVER (PARTITION BY c.flake_id ORDER BY
                   CASE WHEN vf.snapshot_ready_at IS NOT NULL THEN 0 ELSE 1 END,
                   CASE WHEN vf.snapshot_ready_at IS NOT NULL THEN fbcs.position ELSE 0 END,
                   c.commit_timestamp DESC, c.id DESC) AS rn
          FROM visible_flakes vf LEFT JOIN commits c ON c.flake_id = vf.id
          LEFT JOIN flake_branch_commit_snapshot fbcs ON fbcs.flake_id = vf.id AND fbcs.commit_id = c.id
           WHERE c.id IS NULL OR (c.source_archived = false
             AND (vf.snapshot_ready_at IS NULL OR fbcs.commit_id IS NOT NULL))
        ), deployments AS (
          SELECT s.flake_id, v.current_commit_hash,
                 COUNT(DISTINCT s.id)::bigint AS system_count,
                 array_agg(DISTINCT s.hostname ORDER BY s.hostname) AS systems
          FROM systems s JOIN view_system_deployment_status v ON v.hostname = s.hostname
          WHERE v.current_commit_hash IS NOT NULL
            AND ($3::uuid IS NULL OR EXISTS (SELECT 1 FROM user_environment_memberships uem WHERE uem.user_id = $3 AND uem.environment_id = s.environment_id))
          GROUP BY s.flake_id, v.current_commit_hash
        ), builds AS (
          SELECT d.commit_id, CASE
            WHEN COUNT(*) FILTER (WHERE bj.status IN ('building', 'cancelling')) > 0 THEN 'building'
            WHEN COUNT(*) FILTER (WHERE bj.status = 'queued') > 0 THEN 'queued'
            WHEN COUNT(*) FILTER (WHERE bj.status = 'failed') > 0 THEN 'failed'
            WHEN COUNT(*) FILTER (WHERE bj.status = 'success') > 0 THEN 'complete'
            ELSE NULL END AS build_status
          FROM build_jobs bj JOIN derivations d ON d.id = bj.derivation_id
          WHERE d.commit_id IN (SELECT commit_id FROM ranked WHERE rn <= $2)
            AND ($3::uuid IS NULL OR (
              (bj.environment_id IS NOT NULL AND EXISTS (
                SELECT 1 FROM user_environment_memberships uem
                WHERE uem.user_id = $3 AND uem.environment_id = bj.environment_id
              )) OR (bj.environment_id IS NULL AND EXISTS (
                SELECT 1 FROM systems s
                JOIN user_environment_memberships uem ON uem.environment_id = s.environment_id
                WHERE uem.user_id = $3
                  AND (s.hostname = d.derivation_target OR s.system_configuration_name = d.derivation_target)
              ))
            ))
          GROUP BY d.commit_id
        )
        SELECT r.flake_id, r.flake_name, r.repo_url, c.id, c.git_commit_hash AS hash,
               c.message, c.author, c.commit_timestamp AS committed_at,
               COALESCE(dp.system_count, 0) AS system_count,
               COALESCE(dp.systems, ARRAY[]::text[]) AS systems,
               r.commits_behind, b.build_status, c.evaluation_status, c.evaluation_error_message
        FROM ranked r LEFT JOIN commits c ON c.id = r.commit_id
        LEFT JOIN deployments dp ON dp.flake_id = r.flake_id AND dp.current_commit_hash = c.git_commit_hash
        LEFT JOIN builds b ON b.commit_id = c.id
        WHERE r.rn <= $2
        ORDER BY lower(r.flake_name), r.rn"#,
    )
    .bind(&flake_filter)
    .bind(max_commits_per_flake)
    .bind(user_id)
    .fetch_all(pool)
    .await?;

    let mut timelines = Vec::new();
    for row in rows {
        if timelines
            .last()
            .map(|timeline: &FlakeTimeline| timeline.flake_id)
            != Some(row.flake_id)
        {
            timelines.push(FlakeTimeline {
                flake_id: row.flake_id,
                flake_name: row.flake_name.clone(),
                repo_url: row.repo_url.clone(),
                commits: Vec::new(),
            });
        }
        let build_status = row.build_status.as_deref().map(|status| match status {
            "queued" => BuildStatus::Queued,
            "building" => BuildStatus::Building,
            "failed" => BuildStatus::Failed,
            "complete" => BuildStatus::Complete,
            _ => BuildStatus::Idle,
        });
        if let (Some(id), Some(hash), Some(committed_at), Some(commits_behind)) =
            (row.id, row.hash, row.committed_at, row.commits_behind)
        {
            let timeline_index = timelines.len() - 1;
            timelines[timeline_index].commits.push(FlakeCommit {
                id,
                hash,
                message: row.message.unwrap_or_default(),
                author: row.author.unwrap_or_default(),
                committed_at,
                system_count: row.system_count,
                commits_behind,
                systems: row.systems,
                system_paths: Vec::new(),
                build_status,
                evaluation_status: row.evaluation_status,
                evaluation_error_message: row.evaluation_error_message,
                metadata: None,
            });
        }
    }
    Ok(timelines)
}

/// Fetch flake timelines for flakes view (nixosConfigurations in flake).
///
/// Returns up to `max_commits_per_flake` most recent commits for each flake.
///
/// **Set-based, database-only, single round trip** (TASK-397): uses a CTE with
/// `ROW_NUMBER() OVER (PARTITION BY flake_id ...)` to apply the per-flake limit
/// across all requested flakes in one query, eliminating the previous 1+N loop.
///
/// Ordering uses the branch-commit snapshot (position) when `snapshot_ready_at`
/// IS NOT NULL, falling back to `(commit_timestamp DESC, id DESC)` otherwise.
///
/// # Errors
///
/// Returns an error when PostgreSQL cannot load the visible timeline rows.
pub async fn fetch_flake_timelines(
    pool: &PgPool,
    max_commits_per_flake: i64,
    flake_ids: Option<&[i32]>,
    visibility_user: Option<Uuid>,
) -> Result<Vec<FlakeTimeline>> {
    let flake_filter: Option<Vec<i32>> = flake_ids.map(|ids| ids.to_vec());

    #[derive(sqlx::FromRow)]
    struct FlakeCommitRow {
        flake_id: i32,
        flake_name: String,
        repo_url: String,
        id: i32,
        git_commit_hash: String,
        commit_timestamp: chrono::DateTime<chrono::Utc>,
        message: Option<String>,
        author: Option<String>,
        system_count: i64,
        systems: Vec<String>,
        commits_behind: i64,
        build_status: Option<String>,
        evaluation_status: Option<String>,
        evaluation_error_message: Option<String>,
        total_systems: Option<i32>,
        systems_passed_policy: Option<i32>,
        systems_failed_policy_strict: Option<i32>,
        systems_failed_policy_non_strict: Option<i32>,
        has_nix_eval_error: Option<bool>,
        has_policy_failures: Option<bool>,
        all_systems_passed: Option<bool>,
    }

    // Single set-based query for all flakes.
    //
    // Structure:
    //  1. ranked CTE: assigns per-flake sort position using a window function,
    //     respecting snapshot ordering when available and timestamp ordering as
    //     fallback. Snapshot flakes exclude commits not in the snapshot via the
    //     LEFT JOIN filter.
    //  2. build_agg CTE: pre-aggregates build-job status by commit_id across
    //     all selected commits in one pass — eliminates the correlated
    //     build_jobs subquery that previously fired per-commit-row.
    //  3. Outer SELECT: joins ranked + build_agg + cache tables, applies the
    //     per-flake LIMIT via WHERE rn <= $2.
    let rows = sqlx::query_as::<_, FlakeCommitRow>(
        r#"
        WITH ranked AS (
            SELECT
                f.id          AS flake_id,
                f.name        AS flake_name,
                f.repo_url    AS repo_url,
                c.id          AS commit_id,
                CASE
                    WHEN f.snapshot_ready_at IS NOT NULL THEN fbcs.position::bigint
                    ELSE ROW_NUMBER() OVER (
                        PARTITION BY c.flake_id
                        ORDER BY c.commit_timestamp DESC, c.id DESC
                    ) - 1
                END AS commits_behind,
                ROW_NUMBER() OVER (
                    PARTITION BY c.flake_id
                    ORDER BY
                        CASE WHEN f.snapshot_ready_at IS NOT NULL THEN 0 ELSE 1 END,
                        CASE WHEN f.snapshot_ready_at IS NOT NULL
                             THEN fbcs.position ELSE 0 END ASC,
                        c.commit_timestamp DESC,
                        c.id DESC
                ) AS rn
            FROM flakes f
            JOIN commits c ON c.flake_id = f.id
            LEFT JOIN flake_branch_commit_snapshot fbcs
                ON fbcs.commit_id = c.id AND fbcs.flake_id = f.id
            WHERE f.deleted_at IS NULL
              AND c.source_archived = false
              AND ($1::int[] IS NULL OR f.id = ANY($1))
              AND (f.snapshot_ready_at IS NULL OR fbcs.commit_id IS NOT NULL)
              AND ($3::uuid IS NULL OR EXISTS (
                  SELECT 1 FROM systems visible_system
                  JOIN user_environment_memberships membership
                    ON membership.environment_id = visible_system.environment_id
                   AND membership.user_id = $3
                  WHERE visible_system.flake_id = f.id
                    AND visible_system.is_active = TRUE
              ))
        ),
        build_agg AS (
            SELECT
                d.commit_id,
                CASE
                    WHEN COUNT(*) FILTER (WHERE bj.status = 'building') > 0 THEN 'building'
                    WHEN COUNT(*) FILTER (WHERE bj.status = 'queued')   > 0 THEN 'queued'
                    WHEN COUNT(*) FILTER (WHERE bj.status = 'failed')   > 0 THEN 'failed'
                    WHEN COUNT(*) FILTER (WHERE bj.status = 'success')  > 0 THEN 'complete'
                    ELSE NULL
                END AS build_status
            FROM build_jobs bj
            JOIN derivations d ON d.id = bj.derivation_id
            WHERE d.commit_id IN (SELECT commit_id FROM ranked WHERE rn <= $2)
              AND ($3::uuid IS NULL OR (
                  (bj.environment_id IS NOT NULL AND EXISTS (
                      SELECT 1 FROM user_environment_memberships membership
                      WHERE membership.user_id = $3
                        AND membership.environment_id = bj.environment_id
                  )) OR (bj.environment_id IS NULL AND EXISTS (
                      SELECT 1 FROM systems visible_system
                      JOIN user_environment_memberships membership
                        ON membership.environment_id = visible_system.environment_id
                      WHERE membership.user_id = $3
                        AND (visible_system.hostname = d.derivation_target
                             OR visible_system.system_configuration_name = d.derivation_target)
                  ))
              ))
            GROUP BY d.commit_id
        ),
        managed_systems AS (
            SELECT s.flake_id,
                   COUNT(DISTINCT s.id)::bigint AS system_count,
                   array_agg(
                       DISTINCT COALESCE(NULLIF(btrim(s.system_configuration_name), ''), s.hostname)
                       ORDER BY COALESCE(NULLIF(btrim(s.system_configuration_name), ''), s.hostname)
                   ) AS systems
            FROM systems s
            WHERE s.is_active = TRUE
              AND ($3::uuid IS NULL OR EXISTS (
                  SELECT 1 FROM user_environment_memberships membership
                  WHERE membership.user_id = $3
                    AND membership.environment_id = s.environment_id
              ))
            GROUP BY s.flake_id
        )
        SELECT
            r.flake_id,
            r.flake_name,
            r.repo_url,
            c.id,
            c.git_commit_hash,
            c.commit_timestamp,
            c.message,
            c.author,
            COALESCE(managed.system_count, 0)::bigint AS system_count,
            COALESCE(managed.systems, ARRAY[]::text[]) AS systems,
            r.commits_behind,
            ba.build_status,
            c.evaluation_status,
            c.evaluation_error_message,
            CASE WHEN $3::uuid IS NULL THEN cmc.total_systems ELSE NULL END AS total_systems,
            CASE WHEN $3::uuid IS NULL THEN cmc.systems_passed_policy ELSE NULL END AS systems_passed_policy,
            CASE WHEN $3::uuid IS NULL THEN cmc.systems_failed_policy_strict ELSE NULL END AS systems_failed_policy_strict,
            CASE WHEN $3::uuid IS NULL THEN cmc.systems_failed_policy_non_strict ELSE NULL END AS systems_failed_policy_non_strict,
            CASE WHEN $3::uuid IS NULL THEN cmc.has_nix_eval_error ELSE NULL END AS has_nix_eval_error,
            CASE WHEN $3::uuid IS NULL THEN cmc.has_policy_failures ELSE NULL END AS has_policy_failures,
            CASE WHEN $3::uuid IS NULL THEN cmc.all_systems_passed ELSE NULL END AS all_systems_passed
        FROM ranked r
        JOIN commits c ON c.id = r.commit_id
        LEFT JOIN build_agg ba ON ba.commit_id = r.commit_id
        LEFT JOIN commit_artifacts_cache cac ON cac.commit_id = r.commit_id
        LEFT JOIN commit_metadata_cache cmc ON cmc.commit_id = r.commit_id
        LEFT JOIN managed_systems managed ON managed.flake_id = r.flake_id
        WHERE r.rn <= $2
        ORDER BY r.flake_name ASC, r.rn ASC
        "#,
    )
    .bind(&flake_filter)
    .bind(max_commits_per_flake)
    .bind(visibility_user)
    .fetch_all(pool)
    .await?;

    // Group rows into FlakeTimeline per flake, preserving query order.
    let mut timelines: Vec<FlakeTimeline> = Vec::new();
    let mut flake_index: std::collections::HashMap<i32, usize> = std::collections::HashMap::new();

    for row in rows {
        let build_status = row.build_status.as_deref().map(|s| match s {
            "queued" => BuildStatus::Queued,
            "building" => BuildStatus::Building,
            "failed" => BuildStatus::Failed,
            "complete" => BuildStatus::Complete,
            _ => BuildStatus::Idle,
        });

        let metadata = match (
            row.total_systems,
            row.systems_passed_policy,
            row.systems_failed_policy_strict,
            row.systems_failed_policy_non_strict,
            row.has_nix_eval_error,
            row.has_policy_failures,
            row.all_systems_passed,
        ) {
            (
                Some(total_systems),
                Some(systems_passed_policy),
                Some(systems_failed_policy_strict),
                Some(systems_failed_policy_non_strict),
                Some(has_nix_eval_error),
                Some(has_policy_failures),
                Some(all_systems_passed),
            ) => Some(CommitMetadata {
                total_systems,
                systems_passed_policy,
                systems_failed_policy_strict,
                systems_failed_policy_non_strict,
                has_nix_eval_error,
                has_policy_failures,
                all_systems_passed,
            }),
            _ => None,
        };

        let commit = FlakeCommit {
            id: row.id,
            hash: row.git_commit_hash,
            message: row.message.unwrap_or_default(),
            author: row.author.unwrap_or_default(),
            committed_at: row.commit_timestamp,
            system_count: row.system_count,
            commits_behind: row.commits_behind,
            systems: row.systems,
            system_paths: Vec::new(),
            build_status,
            evaluation_status: row.evaluation_status,
            evaluation_error_message: row.evaluation_error_message,
            metadata,
        };

        let idx = if let Some(&i) = flake_index.get(&row.flake_id) {
            i
        } else {
            let i = timelines.len();
            flake_index.insert(row.flake_id, i);
            timelines.push(FlakeTimeline {
                flake_id: row.flake_id,
                flake_name: row.flake_name,
                repo_url: row.repo_url,
                commits: Vec::new(),
            });
            i
        };
        timelines[idx].commits.push(commit);
    }

    Ok(timelines)
}

// ---------------------------------------------------------------------------
// Branch-commit snapshot queries (TASK-397)
// ---------------------------------------------------------------------------

/// Atomically replace the branch-commit snapshot for a flake.
///
/// Deletes all existing snapshot rows for `flake_id`, inserts new rows from
/// `commit_ids` (in order, where index 0 = HEAD), and sets `snapshot_ready_at`.
/// `observed_at` is set to `now()` for every row — this is the observation
/// timestamp, not the commit authorship timestamp.
///
/// Must be called inside an open transaction so readers never see a partial
/// or empty snapshot. If `commit_ids` is empty the snapshot is cleared but
/// `snapshot_ready_at` is still set (indicating an empty tracked branch
/// has been validated).
pub async fn replace_flake_branch_snapshot(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    flake_id: i32,
    commit_ids: &[i32],
) -> Result<()> {
    // Delete existing snapshot rows for this flake
    sqlx::query("DELETE FROM flake_branch_commit_snapshot WHERE flake_id = $1")
        .bind(flake_id)
        .execute(&mut **tx)
        .await
        .context("Failed to delete old branch snapshot")?;

    // Insert new snapshot rows. observed_at is always now() — this is the
    // server observation timestamp, not the commit authorship timestamp.
    for (position, commit_id) in commit_ids.iter().enumerate() {
        let pos = position as i32;
        sqlx::query(
            r#"
            INSERT INTO flake_branch_commit_snapshot (flake_id, commit_id, position, observed_at)
            VALUES ($1, $2, $3, now())
            ON CONFLICT (flake_id, commit_id) DO UPDATE
                SET position = EXCLUDED.position,
                    observed_at = now()
            "#,
        )
        .bind(flake_id)
        .bind(commit_id)
        .bind(pos)
        .execute(&mut **tx)
        .await
        .context("Failed to insert branch snapshot row")?;
    }

    // Mark snapshot as ready
    sqlx::query(
        r#"
        UPDATE flakes
        SET snapshot_ready_at = COALESCE(snapshot_ready_at, now())
        WHERE id = $1
        "#,
    )
    .bind(flake_id)
    .execute(&mut **tx)
    .await
    .context("Failed to set snapshot_ready_at")?;

    Ok(())
}

/// Replace the branch snapshot for a flake using a connection (auto-transaction).
///
/// Opens its own short transaction. Prefer `replace_flake_branch_snapshot` when
/// the caller already has a transaction.
pub async fn replace_flake_branch_snapshot_standalone(
    pool: &PgPool,
    flake_id: i32,
    commit_ids: &[i32],
) -> Result<()> {
    let mut tx = pool.begin().await?;
    replace_flake_branch_snapshot(&mut tx, flake_id, commit_ids).await?;
    tx.commit().await?;
    Ok(())
}

/// Read the ordered branch snapshot for a flake.
///
/// Returns rows ordered by position ascending (position 0 = HEAD).
pub async fn read_flake_branch_snapshot(
    pool: &PgPool,
    flake_id: i32,
    limit: i64,
) -> Result<Vec<BranchCommitSnapshot>> {
    let rows = sqlx::query_as::<_, BranchCommitSnapshot>(
        r#"
        SELECT flake_id, commit_id, position, observed_at
        FROM flake_branch_commit_snapshot
        WHERE flake_id = $1
        ORDER BY position ASC
        LIMIT $2
        "#,
    )
    .bind(flake_id)
    .bind(limit)
    .fetch_all(pool)
    .await?;

    Ok(rows)
}

/// Check whether a flake has a ready branch snapshot.
///
/// Returns `true` if `snapshot_ready_at` is set (migration 0178 has populated
/// at least one snapshot for this flake).
pub async fn is_flake_snapshot_ready(pool: &PgPool, flake_id: i32) -> Result<bool> {
    let ready = sqlx::query_scalar::<_, Option<chrono::DateTime<chrono::Utc>>>(
        r#"
        SELECT snapshot_ready_at
        FROM flakes
        WHERE id = $1 AND deleted_at IS NULL
        "#,
    )
    .bind(flake_id)
    .fetch_optional(pool)
    .await?
    .flatten()
    .is_some();

    Ok(ready)
}

/// Clear the branch snapshot for a flake and reset `snapshot_ready_at`.
///
/// Used when the tracked source identity changes (repo_url or branch) so
/// the old snapshot is no longer authoritative. The next successful sync
/// will populate a fresh snapshot.
pub async fn clear_flake_branch_snapshot(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    flake_id: i32,
) -> Result<()> {
    sqlx::query("DELETE FROM flake_branch_commit_snapshot WHERE flake_id = $1")
        .bind(flake_id)
        .execute(&mut **tx)
        .await
        .context("Failed to clear branch snapshot")?;

    sqlx::query("UPDATE flakes SET snapshot_ready_at = NULL WHERE id = $1")
        .bind(flake_id)
        .execute(&mut **tx)
        .await
        .context("Failed to reset snapshot_ready_at")?;

    Ok(())
}

#[cfg(test)]
mod task_397_tests {
    use super::*;
    use chrono::{Duration, Utc};

    #[sqlx::test]
    #[ignore = "requires a test database role with CREATE DATABASE privileges"]
    async fn set_based_registry_and_timeline_queries_use_snapshot_order(pool: PgPool) {
        let flake = insert_flake(
            &pool,
            "task-397-test",
            "https://example.invalid/task-397-test.git",
            "main",
            "cf_systems_only",
        )
        .await
        .expect("insert test flake");

        let now = Utc::now();
        let mut commit_ids = Vec::new();
        for (offset, hash) in [(2, "oldest"), (1, "middle"), (0, "newest")] {
            let id = sqlx::query_scalar::<_, i32>(
                r#"
                INSERT INTO commits (
                    flake_id, git_commit_hash, commit_timestamp, message, author
                )
                VALUES ($1, $2, $3, $4, 'Test Author')
                RETURNING id
                "#,
            )
            .bind(flake.id)
            .bind(hash)
            .bind(now - Duration::minutes(offset))
            .bind(format!("Commit {hash}"))
            .fetch_one(&pool)
            .await
            .expect("insert test commit");
            commit_ids.push(id);
        }

        let fallback = fetch_flake_timelines(&pool, 2, Some(&[flake.id]), None)
            .await
            .expect("fetch fallback timeline");
        assert_eq!(fallback.len(), 1);
        assert_eq!(fallback[0].commits.len(), 2);
        assert_eq!(fallback[0].commits[0].hash, "newest");
        assert_eq!(fallback[0].commits[1].hash, "middle");

        // Deliberately make snapshot order differ from timestamp order.
        // Snapshot says: oldest (pos 0) → newest (pos 1).
        // Timestamp order would be: newest → middle → oldest.
        replace_flake_branch_snapshot_standalone(&pool, flake.id, &[commit_ids[0], commit_ids[2]])
            .await
            .expect("replace snapshot");

        let snapshot = fetch_flake_timelines(&pool, 10, Some(&[flake.id]), None)
            .await
            .expect("fetch snapshot timeline");
        assert_eq!(snapshot.len(), 1);
        assert_eq!(snapshot[0].commits.len(), 2);
        assert_eq!(snapshot[0].commits[0].hash, "oldest");
        assert_eq!(snapshot[0].commits[0].commits_behind, 0);
        assert_eq!(snapshot[0].commits[1].hash, "newest");
        assert_eq!(snapshot[0].commits[1].commits_behind, 1);

        let registry = list_flake_registry(&pool, None)
            .await
            .expect("fetch enriched registry");
        let item = registry
            .iter()
            .find(|item| item.id == flake.id)
            .expect("registry item");
        assert_eq!(item.latest_commit_hash.as_deref(), Some("oldest"));
        assert_eq!(item.total_commit_count, 2);

        sqlx::query("UPDATE commits SET source_archived = true WHERE id = $1")
            .bind(commit_ids[0])
            .execute(&pool)
            .await
            .expect("archive former snapshot head");
        let active_snapshot = fetch_flake_timelines(&pool, 10, Some(&[flake.id]), None)
            .await
            .expect("fetch active-only snapshot timeline");
        assert_eq!(active_snapshot[0].commits.len(), 1);
        assert_eq!(active_snapshot[0].commits[0].hash, "newest");
        let active_registry = list_flake_registry(&pool, None)
            .await
            .expect("fetch active-only registry");
        let active_item = active_registry
            .iter()
            .find(|item| item.id == flake.id)
            .expect("active registry item");
        assert_eq!(active_item.latest_commit_hash.as_deref(), Some("newest"));
        assert_eq!(active_item.total_commit_count, 1);

        let unrelated_user = Uuid::new_v4();
        assert!(
            list_flake_registry(&pool, Some(unrelated_user))
                .await
                .expect("fetch membership-filtered registry")
                .is_empty()
        );
        assert!(
            fetch_flake_timelines(&pool, 10, Some(&[flake.id]), Some(unrelated_user))
                .await
                .expect("fetch membership-filtered timeline")
                .is_empty()
        );
    }

    /// Regression test for the create-path stale-conflict race: a row
    /// resolved by `create_or_mutate_flake`'s unlocked `repo_url` lookup
    /// must NOT be forcibly reset back to the looked-up `repo_url` if a
    /// concurrent request already moved it elsewhere before the lock was
    /// acquired. `mutate_conflicting_flake_locked` must detect this and
    /// report `Stale` without touching the row.
    #[sqlx::test]
    #[ignore = "requires a test database role with CREATE DATABASE privileges"]
    async fn mutate_conflicting_flake_locked_detects_stale_repo_url(pool: PgPool) {
        // Simulates: create(A) resolved this row's id via an unlocked
        // repo_url lookup...
        let flake = insert_flake(
            &pool,
            "stale-conflict-test",
            "https://example.invalid/stale-conflict-a.git",
            "main",
            "cf_systems_only",
        )
        .await
        .expect("insert test flake");

        // ...then, before the create path acquired the lock, a concurrent
        // update moved the SAME row to a different repo_url and committed.
        let mut tx = pool.begin().await.expect("begin tx");
        reset_flake_source(
            &mut tx,
            flake.id,
            "stale-conflict-test",
            "https://example.invalid/stale-conflict-b.git",
            "main",
            "cf_systems_only",
        )
        .await
        .expect("simulate concurrent update to repo B");
        tx.commit().await.expect("commit simulated update");

        // The create path's conflict-resolution mutation now runs, still
        // holding the STALE id + the ORIGINAL repo_url ("A") it resolved
        // before the concurrent update above.
        let result = mutate_conflicting_flake_locked(
            &pool,
            flake.id,
            "https://example.invalid/stale-conflict-a.git",
            "stale-conflict-test",
            "main",
            "cf_systems_only",
        )
        .await
        .expect("mutate_conflicting_flake_locked should not error");

        assert!(
            matches!(result, ConflictMutation::Stale),
            "expected Stale when the row's repo_url no longer matches the conflict lookup"
        );

        // The row must be untouched: still on repo B, not reset back to A.
        let current = get_flake_by_id(&pool, flake.id)
            .await
            .expect("row must still exist and be readable");
        assert_eq!(
            current.repo_url, "https://example.invalid/stale-conflict-b.git",
            "stale conflict resolution must not clobber the concurrently-updated row"
        );
    }

    /// End-to-end regression test for the exact scenario from review: a
    /// create request for repo A resolves an existing row via conflict,
    /// a concurrent update moves that SAME row to repo B and commits, then
    /// the create request resumes. It must NOT reclaim the row for A;
    /// instead it must create a distinct row for A, leaving the original
    /// row on B untouched.
    #[sqlx::test]
    #[ignore = "requires a test database role with CREATE DATABASE privileges"]
    async fn create_or_mutate_flake_does_not_clobber_concurrently_moved_row(pool: PgPool) {
        let repo_a = "https://example.invalid/race-repo-a.git";
        let repo_b = "https://example.invalid/race-repo-b.git";

        // Initial state: flake row owns repo A (as if an earlier
        // create_or_mutate_flake(..., repo_a, ...) had already resolved and
        // returned this row).
        let original = insert_flake(&pool, "race-test", repo_a, "main", "cf_systems_only")
            .await
            .expect("insert initial flake on repo A");

        // Concurrent update: moves the SAME row from A to B and commits,
        // simulating update_flake_handler racing ahead of a create(A) that
        // already captured this row's id via its unlocked conflict lookup.
        let mut tx = pool.begin().await.expect("begin tx");
        reset_flake_source(
            &mut tx,
            original.id,
            "race-test",
            repo_b,
            "main",
            "cf_systems_only",
        )
        .await
        .expect("simulate concurrent update moving row to repo B");
        tx.commit().await.expect("commit simulated update");

        // The create(A) request "resumes": at this point repo A is free
        // again (the only row that owned it moved to B), so
        // create_or_mutate_flake must retry past the stale conflict and
        // insert a brand-new, distinct row for A.
        let created =
            create_or_mutate_flake(&pool, "race-test-2", repo_a, "main", "cf_systems_only")
                .await
                .expect("create_or_mutate_flake must succeed by creating a distinct row");

        assert_ne!(
            created.id, original.id,
            "a distinct row must be created for repo A, not the row that moved to B"
        );
        assert_eq!(created.repo_url, repo_a);

        // The original row must remain exactly as the concurrent update
        // left it: untouched, still on repo B.
        let original_after = get_flake_by_id(&pool, original.id)
            .await
            .expect("original row must still exist");
        assert_eq!(
            original_after.repo_url, repo_b,
            "original row must remain on repo B, not be reclaimed for repo A"
        );
    }
}

/// Round 11 regression tests for flake lifecycle attention-resolution and
/// lock ordering. Uses the repository's isolated live database
/// (`DATABASE_URL`), not `#[sqlx::test]`'s ephemeral-database mechanism
/// (which requires a CREATE DATABASE-privileged role unavailable in this
/// environment) — matching the pattern used in `queries::attention` and
/// `flake::commits`'s own test modules.
///
/// Run with:
///   DATABASE_URL=postgres://crystal_forge:password@localhost:3042/crystal_forge \
///     cargo test -p cf-server --lib queries::flakes::attention_lifecycle_tests -- --ignored
#[cfg(test)]
mod attention_lifecycle_tests {
    use super::*;

    async fn test_pool() -> PgPool {
        PgPool::connect(
            &std::env::var("DATABASE_URL").expect("DATABASE_URL must be set for DB tests"),
        )
        .await
        .expect("failed to connect to test database")
    }

    async fn insert_throwaway_flake(pool: &PgPool) -> i32 {
        let short = uuid::Uuid::new_v4().simple().to_string()[..12].to_string();
        sqlx::query_scalar::<_, i32>(
            "INSERT INTO flakes (name, repo_url, branch) VALUES ($1, $2, 'main') RETURNING id",
        )
        .bind(format!("att-lifecycle-flake-{short}"))
        .bind(format!(
            "https://git.example/att-lifecycle-flake-{short}.git"
        ))
        .fetch_one(pool)
        .await
        .expect("failed to insert throwaway test flake")
    }

    async fn insert_open_occurrence(pool: &PgPool, flake_id: i32) -> uuid::Uuid {
        let occurrence_id = uuid::Uuid::new_v4();
        sqlx::query(
            "INSERT INTO attention_occurrences (id, category, subject_type, subject_id, source_occurrence_key, opened_at, last_observed_at, metadata) \
             VALUES ($1, 'flakes', 'flake_sync', $2, $3, now(), now(), $4::jsonb)",
        )
        .bind(occurrence_id)
        .bind(flake_id.to_string())
        .bind(format!("flake:{flake_id}:{}", uuid::Uuid::new_v4()))
        .bind(serde_json::json!({"reason": "sync_error"}))
        .execute(pool)
        .await
        .expect("failed to insert open attention occurrence");
        occurrence_id
    }

    async fn occurrence_resolved_at(
        pool: &PgPool,
        occurrence_id: uuid::Uuid,
    ) -> Option<chrono::DateTime<chrono::Utc>> {
        sqlx::query_scalar("SELECT resolved_at FROM attention_occurrences WHERE id = $1")
            .bind(occurrence_id)
            .fetch_one(pool)
            .await
            .unwrap()
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn soft_delete_flake_resolves_open_attention_occurrences() {
        let pool = test_pool().await;
        let flake_id = insert_throwaway_flake(&pool).await;
        let occurrence_id = insert_open_occurrence(&pool, flake_id).await;

        soft_delete_flake(&pool, flake_id)
            .await
            .expect("soft delete should succeed");

        assert!(
            occurrence_resolved_at(&pool, occurrence_id).await.is_some(),
            "soft_delete_flake must resolve open flake attention occurrences"
        );

        let _ = sqlx::query("DELETE FROM attention_occurrences WHERE subject_id = $1")
            .bind(flake_id.to_string())
            .execute(&pool)
            .await;
        let _ = sqlx::query("DELETE FROM flakes WHERE id = $1")
            .bind(flake_id)
            .execute(&pool)
            .await;
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn cascade_delete_flake_resolves_open_attention_occurrences() {
        let pool = test_pool().await;
        let flake_id = insert_throwaway_flake(&pool).await;
        let occurrence_id = insert_open_occurrence(&pool, flake_id).await;

        let mut tx = pool.begin().await.expect("begin tx");
        cascade_delete_flake(&mut tx, flake_id)
            .await
            .expect("cascade delete should succeed");
        tx.commit().await.expect("commit cascade delete");

        assert!(
            occurrence_resolved_at(&pool, occurrence_id).await.is_some(),
            "cascade_delete_flake must resolve open flake attention occurrences"
        );

        let _ = sqlx::query("DELETE FROM attention_occurrences WHERE subject_id = $1")
            .bind(flake_id.to_string())
            .execute(&pool)
            .await;
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn delete_flake_by_id_resolves_open_attention_occurrences() {
        let pool = test_pool().await;
        let flake_id = insert_throwaway_flake(&pool).await;
        let occurrence_id = insert_open_occurrence(&pool, flake_id).await;

        delete_flake_by_id(&pool, flake_id)
            .await
            .expect("hard delete should succeed");

        assert!(
            occurrence_resolved_at(&pool, occurrence_id).await.is_some(),
            "delete_flake_by_id must resolve open flake attention occurrences \
             before the row is deleted (round 11: this function previously \
             acquired no locks and did not resolve attention at all)"
        );

        let _ = sqlx::query("DELETE FROM attention_occurrences WHERE subject_id = $1")
            .bind(flake_id.to_string())
            .execute(&pool)
            .await;
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn reset_flake_source_resolves_occurrences_and_clears_last_synced_at() {
        // Regression test for round 10/11: changing repo_url/branch must
        // unconditionally close the prior attention episode and clear
        // last_synced_at, so an incident from the old source identity
        // cannot be silently inherited by the new one.
        let pool = test_pool().await;
        let flake_id = insert_throwaway_flake(&pool).await;
        let occurrence_id = insert_open_occurrence(&pool, flake_id).await;

        sqlx::query("UPDATE flakes SET last_synced_at = now() WHERE id = $1")
            .bind(flake_id)
            .execute(&pool)
            .await
            .unwrap();

        let mut tx = pool.begin().await.expect("begin tx");
        reset_flake_source(
            &mut tx,
            flake_id,
            "reset-test",
            "https://git.example/reset-test-new.git",
            "new-branch",
            "cf_systems_only",
        )
        .await
        .expect("reset_flake_source should succeed");
        tx.commit().await.expect("commit reset");

        assert!(
            occurrence_resolved_at(&pool, occurrence_id).await.is_some(),
            "reset_flake_source must resolve the prior source's open attention occurrence"
        );

        let last_synced_at: Option<chrono::DateTime<chrono::Utc>> =
            sqlx::query_scalar("SELECT last_synced_at FROM flakes WHERE id = $1")
                .bind(flake_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(
            last_synced_at.is_none(),
            "reset_flake_source must clear last_synced_at for the new source identity"
        );

        let _ = sqlx::query("DELETE FROM attention_occurrences WHERE subject_id = $1")
            .bind(flake_id.to_string())
            .execute(&pool)
            .await;
        let _ = sqlx::query("DELETE FROM flakes WHERE id = $1")
            .bind(flake_id)
            .execute(&pool)
            .await;
    }

    // ── Round 12: cascade delete resolves derived system/environment occs ──
    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn cascade_delete_flake_resolves_system_and_environment_occurrences() {
        let pool = test_pool().await;
        let flake_id = insert_throwaway_flake(&pool).await;

        // Environment for the system.
        let env_id = uuid::Uuid::new_v4();
        sqlx::query("INSERT INTO environments (id, name) VALUES ($1, $2)")
            .bind(env_id)
            .bind(format!("cascade-test-env-{}", &flake_id))
            .execute(&pool)
            .await
            .unwrap();

        // System belonging to the flake.
        let system_id = uuid::Uuid::new_v4();
        sqlx::query(
            "INSERT INTO systems (id, hostname, environment_id, flake_id, is_active, public_key, derivation) \
             VALUES ($1, $2, $3, $4, TRUE, $5, $6)",
        )
        .bind(system_id)
        .bind(format!("cascade-test-{flake_id}"))
        .bind(env_id)
        .bind(flake_id)
        .bind(format!("ssh-ed25519 AAAA-cascade-{flake_id}"))
        .bind("/nix/store/cascade-test-derivation")
        .execute(&pool)
        .await
        .unwrap();

        // Open system occurrence.
        sqlx::query(
            "INSERT INTO attention_occurrences (id, category, subject_type, subject_id, \
             source_occurrence_key, opened_at, last_observed_at, metadata) \
             VALUES ($1, 'systems', 'system_health', $2, $3, now(), now(), $4::jsonb)",
        )
        .bind(uuid::Uuid::new_v4())
        .bind(system_id.to_string())
        .bind(format!(
            "system:{system_id}:offline:{}",
            uuid::Uuid::new_v4()
        ))
        .bind(serde_json::json!({"reason": "offline"}))
        .execute(&pool)
        .await
        .unwrap();

        // Open environment occurrence derived from this system.
        sqlx::query(
            "INSERT INTO attention_occurrences (id, category, subject_type, subject_id, \
             source_occurrence_key, opened_at, last_observed_at, metadata) \
             VALUES ($1, 'environments', 'environment', $2, $3, now(), now(), $4::jsonb)",
        )
        .bind(uuid::Uuid::new_v4())
        .bind(env_id.to_string())
        .bind(format!("environment:{env_id}:{}", uuid::Uuid::new_v4()))
        .bind(serde_json::json!({
            "underlying_system_id": system_id.to_string(),
        }))
        .execute(&pool)
        .await
        .unwrap();

        // Cascade delete.
        let mut tx = pool.begin().await.unwrap();
        cascade_delete_flake(&mut tx, flake_id).await.unwrap();
        tx.commit().await.unwrap();

        // Verify: no open occurrences remain for flakes, systems, or environments.
        let flake_open: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM attention_occurrences \
             WHERE category = 'flakes' AND resolved_at IS NULL",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            flake_open, 0,
            "cascade delete must resolve all flake occurrences"
        );

        let system_open: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM attention_occurrences \
             WHERE category = 'systems' AND subject_id = $1 AND resolved_at IS NULL",
        )
        .bind(system_id.to_string())
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            system_open, 0,
            "cascade delete must resolve all system occurrences for deleted systems"
        );

        let env_open: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*) FROM attention_occurrences ao
            WHERE ao.category = 'environments'
              AND ao.resolved_at IS NULL
              AND ao.metadata->>'underlying_system_id' = $1
            "#,
        )
        .bind(system_id.to_string())
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            env_open, 0,
            "cascade delete must resolve all environment occurrences for deleted systems"
        );

        // Cleanup.
        let _ = sqlx::query("DELETE FROM attention_occurrences WHERE subject_id = ANY($1)")
            .bind::<Vec<String>>(vec![system_id.to_string(), env_id.to_string()])
            .execute(&pool)
            .await;
        let _ = sqlx::query("DELETE FROM systems WHERE id = $1")
            .bind(system_id)
            .execute(&pool)
            .await;
        let _ = sqlx::query("DELETE FROM environments WHERE id = $1")
            .bind(env_id)
            .execute(&pool)
            .await;
        let _ = sqlx::query("DELETE FROM flakes WHERE id = $1")
            .bind(flake_id)
            .execute(&pool)
            .await;
    }
}
