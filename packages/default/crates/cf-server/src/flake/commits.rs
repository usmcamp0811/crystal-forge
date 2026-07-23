use crate::config;
use crate::derivations::utils::build_flake_reference;
use crate::flake::credentials::FlakeCredentialEnv;
use crate::models::commits::Commit;
use crate::queries::attention;
use crate::queries::commits::{flake_has_commits, insert_commit, insert_commit_with_metadata};
use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use std::collections::HashMap;
use tokio::time::{Duration, sleep, timeout};
use tracing::{debug, error, info, warn};
use url::Url;
use uuid::Uuid;

const GIT_METADATA_TIMEOUT: Duration = Duration::from_secs(10);
const GIT_PROBE_TIMEOUT: Duration = Duration::from_secs(10);
const NIX_CONFIG_EVAL_TIMEOUT: Duration = Duration::from_secs(60);
const INIT_COMMIT_RETRY_ATTEMPTS: usize = 5;
const INIT_COMMIT_RETRY_DELAY: Duration = Duration::from_secs(1);
const HISTORY_REWRITE_ERROR_MARKER: &str = "history_rewrite_detected";

async fn fetch_and_insert_recent_commits_with_retry(
    pool: &PgPool,
    repo_url: &str,
    branch: &str,
    limit: Option<usize>,
) -> Result<Vec<String>> {
    let mut last_err: Option<anyhow::Error> = None;

    for attempt in 1..=INIT_COMMIT_RETRY_ATTEMPTS {
        match fetch_and_insert_recent_commits(pool, repo_url, branch, limit).await {
            Ok(commits) => return Ok(commits),
            Err(err) => {
                last_err = Some(err);

                if attempt < INIT_COMMIT_RETRY_ATTEMPTS {
                    warn!(
                        "⚠️ Commit initialization attempt {}/{} failed for {} (branch {}), retrying in {:?}",
                        attempt,
                        INIT_COMMIT_RETRY_ATTEMPTS,
                        repo_url,
                        branch,
                        INIT_COMMIT_RETRY_DELAY
                    );
                    sleep(INIT_COMMIT_RETRY_DELAY).await;
                }
            }
        }
    }

    Err(last_err.unwrap_or_else(|| anyhow::anyhow!("commit initialization failed")))
}

#[derive(Debug, Clone)]
pub struct GitCommitMetadata {
    pub message: String,
    pub author_name: String,
    pub author_email: Option<String>,
}

/// Fetches the latest commit from a git repository and inserts it into the database
pub async fn fetch_and_insert_latest_commit(
    pool: &PgPool,
    repo_url: &str,
    branch: &str,
) -> Result<Option<String>> {
    let commits = get_commits_with_timestamps(repo_url, branch, Some(1), None).await?;

    let (commit_hash, timestamp) = commits
        .into_iter()
        .next()
        .context("No commits found in repository")?;

    insert_commit(pool, &commit_hash, repo_url, timestamp).await?;

    info!(
        "✅ Inserted latest commit {} for repo {}",
        commit_hash, repo_url
    );
    Ok(Some(commit_hash))
}

/// Fetch up to N recent commits from a git repository and insert them into the database
pub async fn fetch_and_insert_recent_commits(
    pool: &PgPool,
    repo_url: &str,
    branch: &str,
    limit: Option<usize>,
) -> Result<Vec<String>> {
    let commits = get_commits_with_full_metadata(repo_url, branch, limit, None, None).await?;

    let mut inserted = Vec::new();
    for commit_data in commits {
        match insert_commit_with_metadata(
            pool,
            &commit_data.hash,
            repo_url,
            commit_data.timestamp,
            Some(&commit_data.message),
            Some(&commit_data.author),
        )
        .await
        {
            Ok(n) if n > 0 => inserted.push(commit_data.hash),
            Ok(_) => {}
            Err(e) => warn!("Failed to insert commit {}: {}", commit_data.hash, e),
        }
    }

    Ok(inserted)
}

/// Like [`fetch_and_insert_recent_commits`] but loads flake credentials from the DB
/// and injects them into git operations.
pub async fn fetch_and_insert_recent_commits_with_creds(
    pool: &PgPool,
    repo_url: &str,
    branch: &str,
    limit: Option<usize>,
    flake_id: i32,
) -> Result<Vec<String>> {
    let creds = FlakeCredentialEnv::load(pool, flake_id)
        .await
        .unwrap_or_else(|e| {
            warn!("Failed to load credentials for flake {flake_id}: {e:#}");
            None
        });
    let commits =
        get_commits_with_full_metadata(repo_url, branch, limit, None, creds.as_ref()).await?;

    let mut inserted = Vec::new();
    for commit_data in commits {
        match insert_commit_with_metadata(
            pool,
            &commit_data.hash,
            repo_url,
            commit_data.timestamp,
            Some(&commit_data.message),
            Some(&commit_data.author),
        )
        .await
        {
            Ok(n) if n > 0 => inserted.push(commit_data.hash),
            Ok(_) => {}
            Err(e) => warn!("Failed to insert commit {}: {}", commit_data.hash, e),
        }
    }

    Ok(inserted)
}

// TODO: update this to get the last N commits for each flake if we are starting for the first time
/// Initialize commits for all watched flakes that don't have any commits yet
/// This is meant to run once when the server first starts
pub async fn initialize_flake_commits(
    pool: &PgPool,
    watched_flakes: &[crate::config::WatchedFlake],
) -> Result<()> {
    info!(
        "🔄 Initializing commits for {} watched flakes",
        watched_flakes.len()
    );

    for flake in watched_flakes {
        if !flake.auto_poll {
            debug!("⏭️ Skipping {} (auto_poll = false)", flake.name);
            continue;
        }

        // Check if this flake already has commits
        match flake_has_commits(pool, &flake.repo_url).await {
            Ok(true) => {
                debug!("⏭️ Skipping {} (already has commits)", flake.name);
                continue;
            }
            Ok(false) => {
                info!("🔗 Initializing commits for flake: {}", flake.name);
            }
            Err(e) => {
                warn!("❌ Failed to check commits for {}: {}", flake.name, e);
                continue;
            }
        }

        match fetch_and_insert_recent_commits_with_retry(
            pool,
            &flake.repo_url,
            &flake.branch(),
            Some(flake.initial_commit_depth),
        )
        .await
        {
            Ok(commits) => {
                info!(
                    "✅ Successfully initialized {} commits for {} on branch {}",
                    commits.len(),
                    flake.name,
                    flake.branch()
                );
            }
            Err(e) => {
                warn!(
                    "❌ Failed to initialize commits for {}: {} on branch {}",
                    flake.name,
                    e,
                    flake.branch()
                );
            }
        }
    }

    Ok(())
}

/// Sync commits for all watched flakes that have auto_poll enabled (for regular polling).
///
/// `watched_flakes` is a slice of `(WatchedFlake, Option<flake_id>)`.  When a flake_id is
/// present the polling loop loads per-flake credentials from the DB and injects them.
pub async fn sync_all_watched_flakes_commits(
    pool: &PgPool,
    watched_flakes: &[config::WatchedFlake],
) -> Result<u64> {
    sync_all_watched_flakes_commits_inner(pool, watched_flakes, &[]).await
}

/// Like [`sync_all_watched_flakes_commits`] but also takes a parallel slice of DB flake IDs
/// so that per-flake credentials can be loaded.  `flake_ids[i]` corresponds to
/// `watched_flakes[i]`; a value of `None` means the flake has no DB record yet.
pub async fn sync_all_watched_flakes_commits_with_ids(
    pool: &PgPool,
    watched_flakes: &[config::WatchedFlake],
    flake_ids: &[Option<i32>],
) -> Result<u64> {
    sync_all_watched_flakes_commits_inner(pool, watched_flakes, flake_ids).await
}

async fn sync_all_watched_flakes_commits_inner(
    pool: &PgPool,
    watched_flakes: &[config::WatchedFlake],
    flake_ids: &[Option<i32>],
) -> Result<u64> {
    info!(
        "🔄 Syncing commits for {} watched flakes",
        watched_flakes.len()
    );

    let mut total_inserted = 0;

    for (idx, flake) in watched_flakes.iter().enumerate() {
        if !flake.auto_poll {
            debug!("⭐️ Skipping {} (auto_poll = false)", flake.name);
            continue;
        }

        let flake_id_opt = flake_ids.get(idx).copied().flatten();

        info!("🔗 Syncing commits for flake: {}", flake.name);

        let inserted: Result<u64> = if let Some(flake_id) = flake_id_opt {
            sync_flake_recorded(pool, flake_id, &flake.repo_url, &flake.branch())
                .await
                .map(|r| r)
        } else {
            sync_commits_for_repo(pool, &flake.repo_url, &flake.branch())
                .await
                .map(|(count, _hashes)| count)
        };

        match inserted {
            Ok(count) => {
                total_inserted += count;
                if count > 0 {
                    info!("✅ Found {} new commits for {}", count, flake.name);
                } else {
                    debug!("📍 No new commits for {}", flake.name);
                }
            }
            Err(e) => {
                warn!("⚠️ Failed to sync commits for {}: {}", flake.name, e);
            }
        }
    }

    Ok(total_inserted)
}

/// Sync commits for a single flake repository URL.
///
/// Returns `(newly_inserted_count, ordered_git_hashes)` where the hashes are in
/// Git traversal order (HEAD first) from the same git log used for insertion.
pub async fn sync_commits_for_repo(
    pool: &PgPool,
    repo_url: &str,
    branch: &str,
) -> Result<(u64, Vec<String>)> {
    sync_commits_for_repo_inner(pool, repo_url, branch, None).await
}

/// Sync commits for a single flake, loading per-flake credentials from the DB.
///
/// Use this instead of [`sync_commits_for_repo`] when the caller has a `flake_id`
/// available and the flake may require authentication.
pub async fn sync_commits_for_flake(
    pool: &PgPool,
    repo_url: &str,
    branch: &str,
    flake_id: i32,
) -> Result<(u64, Vec<String>)> {
    let creds = FlakeCredentialEnv::load(pool, flake_id)
        .await
        .unwrap_or_else(|e| {
            warn!("Failed to load credentials for flake {flake_id}: {e:#}");
            None
        });
    sync_commits_for_repo_inner(pool, repo_url, branch, creds.as_ref()).await
}

/// Sync commits for a flake and record the sync outcome (syncing → synced|error)
/// on the flake row. Status writes are best-effort and never mask the sync result.
///
/// This is the preferred entry point for all sync call sites when `flake_id` is
/// available — it is the only way to persist real sync-failure information.
///
/// Concurrent attempts are guarded by a per-attempt UUID written to
/// `sync_attempt_id`. The final status write (`synced` or `error`) is
/// conditional on that column still matching — if a newer attempt has already
/// started (and written its own UUID), the stale attempt's status write is
/// silently skipped rather than overwriting the newer result.
pub async fn sync_flake_recorded(
    pool: &PgPool,
    flake_id: i32,
    repo_url: &str,
    branch: &str,
) -> Result<u64> {
    use crate::queries::commits::{SYNC_LOCK_BASE, insert_commit_by_flake_id_tx};

    let attempt_id = Uuid::new_v4();

    // Guard the syncing mark on both id AND the current repo_url/branch.
    // This prevents marking a freshly-reset flake as syncing with stale args.
    let start_result = sqlx::query(
        "UPDATE flakes \
         SET sync_status = 'syncing', last_sync_at = now(), last_sync_error = NULL, \
             sync_attempt_id = $2 \
         WHERE id = $1 AND deleted_at IS NULL \
           AND repo_url = $3 AND branch = $4",
    )
    .bind(flake_id)
    .bind(attempt_id)
    .bind(repo_url)
    .bind(branch)
    .execute(pool)
    .await?;
    if start_result.rows_affected() != 1 {
        bail!("flake {flake_id} not found or repo_url/branch changed before sync could start");
    }

    let creds = FlakeCredentialEnv::load(pool, flake_id)
        .await
        .unwrap_or_else(|e| {
            warn!("Failed to load credentials for flake {flake_id}: {e:#}");
            None
        });

    // Phase 1: Git work — clone, log, force-push detection.  No DB writes.
    let commits = match collect_git_commits(pool, flake_id, repo_url, branch, creds.as_ref()).await
    {
        Ok(c) => c,
        Err(e) => {
            record_sync_error(pool, flake_id, attempt_id, repo_url, &e.to_string()).await;
            return Err(e);
        }
    };

    let ordered_hashes: Vec<String> = commits.iter().map(|c| c.hash.clone()).collect();

    // Phase 2: Acquire the per-flake advisory lock, then perform ALL database
    // mutations (insert commits, replace snapshot, update status) in ONE
    // transaction.  reset_flake_source() acquires the same lock, so this
    // transaction cannot race with a concurrent source reset.
    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(e) => {
            error!("Failed to begin mutation tx (flake {flake_id}): {e:#}");
            record_sync_error(pool, flake_id, attempt_id, repo_url, &e.to_string()).await;
            return Err(e.into());
        }
    };

    // Acquire the per-flake advisory lock (transaction-scoped).
    if let Err(e) = sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(SYNC_LOCK_BASE + i64::from(flake_id))
        .execute(&mut *tx)
        .await
    {
        error!("Failed to acquire advisory lock (flake {flake_id}): {e:#}");
        let _ = tx.rollback().await;
        record_sync_error(pool, flake_id, attempt_id, repo_url, &e.to_string()).await;
        return Err(e.into());
    }

    // Recheck inside the lock: if a reset ran before we got the lock,
    // the attempt_id or branch would have changed.
    let current_state = sqlx::query_as::<_, (String, Option<uuid::Uuid>)>(
        "SELECT branch, sync_attempt_id FROM flakes WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(flake_id)
    .fetch_optional(&mut *tx)
    .await;

    match current_state {
        Ok(Some((cur_branch, cur_attempt))) => {
            if cur_branch != branch || cur_attempt != Some(attempt_id) {
                info!("Flake {flake_id} was reset or superseded before lock; aborting");
                let _ = tx.rollback().await;
                return Ok(0);
            }
        }
        Ok(None) => {
            let _ = tx.rollback().await;
            bail!("Flake {flake_id} disappeared before mutation tx");
        }
        Err(e) => {
            error!("Recheck failed (flake {flake_id}): {e:#}");
            let _ = tx.rollback().await;
            record_sync_error(pool, flake_id, attempt_id, repo_url, &e.to_string()).await;
            return Err(e.into());
        }
    }

    // Pre-filter existing hashes inside the lock so concurrent syncs don't race.
    let candidate_hashes: Vec<&str> = commits.iter().map(|c| c.hash.as_str()).collect();
    let existing: std::collections::HashSet<String> = if candidate_hashes.is_empty() {
        std::collections::HashSet::new()
    } else {
        match sqlx::query_scalar::<_, String>(
            "SELECT git_commit_hash FROM commits WHERE flake_id = $1 AND git_commit_hash = ANY($2)",
        )
        .bind(flake_id)
        .bind(&candidate_hashes)
        .fetch_all(&mut *tx)
        .await
        {
            Ok(rows) => rows.into_iter().collect(),
            Err(e) => {
                error!("Pre-filter query failed (flake {flake_id}): {e:#}");
                let _ = tx.rollback().await;
                record_sync_error(pool, flake_id, attempt_id, repo_url, &e.to_string()).await;
                return Err(e.into());
            }
        }
    };

    // Insert missing commits inside the transaction using flake_id directly.
    let mut inserted_count: u64 = 0;
    for commit_data in &commits {
        if existing.contains(&commit_data.hash) {
            continue;
        }
        match insert_commit_by_flake_id_tx(
            &mut tx,
            flake_id,
            &commit_data.hash,
            commit_data.timestamp,
            Some(&commit_data.message),
            Some(&commit_data.author),
        )
        .await
        {
            Ok(n) => inserted_count += n,
            Err(e) => {
                error!(
                    "Insert failed for {} (flake {flake_id}): {e:#}",
                    commit_data.hash
                );
                let _ = tx.rollback().await;
                record_sync_error(pool, flake_id, attempt_id, repo_url, &e.to_string()).await;
                return Err(e);
            }
        }
    }

    // Resolve ordered hashes to DB IDs inside the tx (committed rows visible now).
    let resolved = match resolve_ordered_ids_tx(&mut tx, flake_id, &ordered_hashes).await {
        Ok(ids) => ids,
        Err(e) => {
            error!("ID resolution failed (flake {flake_id}): {e:#}");
            let _ = tx.rollback().await;
            record_sync_error(pool, flake_id, attempt_id, repo_url, &e.to_string()).await;
            return Err(e);
        }
    };

    // Replace snapshot.
    if let Err(e) =
        crate::queries::flakes::replace_flake_branch_snapshot(&mut tx, flake_id, &resolved).await
    {
        error!("Snapshot publication failed (flake {flake_id}): {e:#}");
        let _ = tx.rollback().await;
        record_sync_error(pool, flake_id, attempt_id, repo_url, &e.to_string()).await;
        return Err(e);
    }

    // Acquire the attention subject lock BEFORE the final status update,
    // still inside this same transaction (which already holds the sync
    // lock). This establishes the fixed lock order used by every flake
    // lifecycle operation -- sync lock, then attention lock, then row
    // mutations -- and, critically, means the status update AND the
    // attention resolution below commit together as one atomic unit.
    //
    // Round 11 finding: `now()` (used for `last_synced_at`) is the
    // transaction START time, not the commit time, so comparing it against
    // a concurrent reconciler's `last_observed_at` cannot reliably prove
    // "this success happened after that observation" -- a reconciler could
    // observe the still-committed 'syncing' state and advance an old
    // occurrence AFTER this transaction started but BEFORE it commits,
    // producing a last_observed_at that is later than this transaction's
    // last_synced_at despite this transaction's commit being the more
    // recent durable fact. Timestamp comparison alone cannot substitute for
    // an actual commit-order guarantee.
    //
    // Acquiring the attention lock here and holding it through commit
    // closes that race structurally rather than via timestamps: any
    // concurrent reconciler that also acquires this lock (all of
    // `reconcile_single_stale_flake`, `transition_flake_attention_to_error_if_current`,
    // and `resolve_flake_attention_if_current` do) must either complete
    // and commit BEFORE this transaction acquires the lock (in which case
    // the resolve below correctly supersedes it), or block until this
    // transaction commits or rolls back (in which case it observes the
    // final, consistent post-commit state -- `sync_status = 'synced'` and
    // no open occurrence -- and its own recheck logic correctly no-ops).
    let attention_lock_key = format!("attention_occurrence:flakes:{flake_id}");
    if let Err(e) = sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(&attention_lock_key)
        .execute(&mut *tx)
        .await
    {
        error!("Failed to acquire attention lock (flake {flake_id}): {e:#}");
        let _ = tx.rollback().await;
        record_sync_error(pool, flake_id, attempt_id, repo_url, &e.to_string()).await;
        return Err(e.into());
    }

    // Update final sync status inside the same transaction, still holding
    // both the sync lock and (now) the attention lock. `last_synced_at` is
    // set unconditionally alongside `last_sync_at` -- unlike `last_sync_at`
    // (overwritten on every attempt, success or failure), `last_synced_at`
    // is ONLY ever written here, so it monotonically advances only on
    // success and survives any later failure. It remains useful as a
    // defense-in-depth signal for `reconcile_single_stale_flake` (whose
    // occurrence reuse decision happens in a separate, later transaction
    // than this one and so cannot rely on this transaction's lock), even
    // though the atomic resolve below is what actually closes the race
    // described above for THIS transaction's own attention resolution.
    let status_update = sqlx::query(
        "UPDATE flakes \
         SET sync_status = 'synced', last_sync_at = now(), last_sync_error = NULL, last_synced_at = now() \
         WHERE id = $1 AND deleted_at IS NULL AND sync_attempt_id = $2",
    )
    .bind(flake_id)
    .bind(attempt_id)
    .execute(&mut *tx)
    .await;

    match status_update {
        Ok(upd) if upd.rows_affected() == 1 => {}
        Ok(_) => {
            // Superseded — a newer attempt was started. Roll back.
            info!("Flake {flake_id} sync was superseded before status commit; aborting");
            let _ = tx.rollback().await;
            return Ok(0);
        }
        Err(e) => {
            error!("Status update failed (flake {flake_id}): {e:#}");
            let _ = tx.rollback().await;
            record_sync_error(pool, flake_id, attempt_id, repo_url, &e.to_string()).await;
            return Err(e.into());
        }
    }

    // Resolve all open flake attention occurrences in the SAME transaction,
    // still holding the attention lock acquired above. No separate
    // "is this attempt still current" recheck is needed here (unlike
    // `resolve_flake_attention_if_current`, which runs in its OWN later
    // transaction and therefore must recheck): the status update just
    // above already proved, within this transaction, that no newer attempt
    // has superseded us, and the attention lock held since before that
    // update guarantees no concurrent transition can race between the
    // update and this resolve.
    if let Err(e) = sqlx::query(
        "UPDATE attention_occurrences SET resolved_at = NOW() \
         WHERE category = 'flakes' AND subject_id = $1 AND resolved_at IS NULL",
    )
    .bind(flake_id.to_string())
    .execute(&mut *tx)
    .await
    {
        error!("Failed to resolve flake attention occurrences (flake {flake_id}): {e:#}");
        let _ = tx.rollback().await;
        record_sync_error(pool, flake_id, attempt_id, repo_url, &e.to_string()).await;
        return Err(e.into());
    }

    // Commit everything: inserts + snapshot + status + attention resolve,
    // all as one atomic unit.
    if let Err(e) = tx.commit().await {
        error!("Commit failed (flake {flake_id}): {e:#}");
        record_sync_error(pool, flake_id, attempt_id, repo_url, &e.to_string()).await;
        return Err(e.into());
    }

    if inserted_count > 0 {
        info!(
            "✅ Synced {} ({}): {} new commits ({} in git log)",
            repo_url,
            branch,
            inserted_count,
            ordered_hashes.len()
        );
    }

    Ok(inserted_count)
}

/// Outcome of an accepted history rewrite.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistoryRewriteOutcome {
    /// The rewrite was applied: stale commits were removed and new ones inserted.
    Applied {
        /// Number of obsolete commit rows deleted.
        deleted_commits: u64,
        /// Number of genuinely new commit rows inserted.
        inserted_commits: u64,
    },
    /// Another sync superseded the rewrite before it could commit.
    Superseded,
}

/// Accept a git history rewrite and re-sync, preserving commit rows whose
/// hashes remain reachable in the accepted snapshot history (with their
/// derivations, caches, evaluation status, and completion timestamps).
///
/// Unlike the general `sync_flake_recorded`, this function:
///
/// 1. Does NOT detect force-push rewrites (the caller has already confirmed).
/// 2. Preserves existing commit rows whose hashes still appear in the
///    snapshot (the newest 500) — their `evaluation_status`, derivations,
///    caches, and completion timestamps remain intact, avoiding redundant
///    re-evaluation.
/// 3. Inserts only genuinely new hashes as `evaluation_status = 'pending'`.
/// 4. Does NOT delete commit rows beyond the snapshot window (round 16
///    review: the 500-entry git log is the snapshot display limit, not the
///    full branch reachability set; historical rows outside this window
///    remain as archived evaluation records until a separate retention
///    policy removes them).
/// 5. Resolves eval/build attention for commits that leave the active
///    snapshot, so archived failures cannot keep or reacquire sidebar
///    attention (round 16 review).
///
/// All mutations (insert, snapshot replace, attention resolve, status update)
/// happen inside a single sync-locked transaction, so the intermediate state
/// where re-synced commits appear pending is never visible to the evaluation
/// worker — see Round 15 review.
///
/// Returns `HistoryRewriteOutcome` so the caller can distinguish an applied
/// rewrite from a superseded one.
pub async fn accept_history_rewrite_and_sync(
    pool: &PgPool,
    flake_id: i32,
    repo_url: &str,
    branch: &str,
) -> Result<HistoryRewriteOutcome> {
    use crate::queries::commits::{SYNC_LOCK_BASE, insert_commit_by_flake_id_tx};

    let attempt_id = Uuid::new_v4();

    // Mark as syncing.  Same guard as sync_flake_recorded.
    // NOTE: every subsequent error path must call record_sync_error to
    // transition from 'syncing' back to 'error'; a bare return without
    // record_sync_error leaves the flake stuck in 'syncing' (round 16).
    let start_result = sqlx::query(
        "UPDATE flakes \
         SET sync_status = 'syncing', last_sync_at = now(), last_sync_error = NULL, \
             sync_attempt_id = $2 \
         WHERE id = $1 AND deleted_at IS NULL \
           AND repo_url = $3 AND branch = $4",
    )
    .bind(flake_id)
    .bind(attempt_id)
    .bind(repo_url)
    .bind(branch)
    .execute(pool)
    .await
    .map_err(|e| {
        error!("Failed to mark flake {flake_id} as syncing for rewrite: {e:#}");
        e
    })?;
    if start_result.rows_affected() != 1 {
        bail!("flake {flake_id} not found or repo_url/branch changed before rewrite sync could start");
    }

    let creds = FlakeCredentialEnv::load(pool, flake_id)
        .await
        .unwrap_or_else(|e| {
            warn!("Failed to load credentials for flake {flake_id}: {e:#}");
            None
        });

    // Phase 1: Git work — clone and log.  No force-push detection needed
    // (the caller has already confirmed they accept the rewrite).
    //
    // The git log is limited to MAX_SNAPSHOT_COMMITS (500) because that is
    // the snapshot display window.  We do NOT derive the delete-reachability
    // set from this window — historical commits beyond the newest 500 are
    // preserved as archived evaluation records (round 16 review).
    let max = MAX_SNAPSHOT_COMMITS as usize;
    let commits = match get_commits_with_full_metadata_and_dir(
        repo_url,
        branch,
        Some(max),
        None,
        creds.as_ref(),
    )
    .await
    {
        Ok((commits, _dir)) => commits,
        Err(e) => {
            record_sync_error(pool, flake_id, attempt_id, repo_url, &e.to_string()).await;
            return Err(e);
        }
    };

    let ordered_hashes: Vec<String> = commits.iter().map(|c| c.hash.clone()).collect();

    // Phase 2: Single locked transaction — insert new, preserve existing.
    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(e) => {
            error!("Failed to begin rewrite mutation tx (flake {flake_id}): {e:#}");
            record_sync_error(pool, flake_id, attempt_id, repo_url, &e.to_string()).await;
            return Err(e.into());
        }
    };

    // Acquire the per-flake advisory lock (transaction-scoped).
    if let Err(e) = sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(SYNC_LOCK_BASE + i64::from(flake_id))
        .execute(&mut *tx)
        .await
    {
        error!("Failed to acquire advisory lock for rewrite (flake {flake_id}): {e:#}");
        let _ = tx.rollback().await;
        record_sync_error(pool, flake_id, attempt_id, repo_url, &e.to_string()).await;
        return Err(e.into());
    }

    // Recheck inside the lock — another process may have superseded us.
    let current_state = sqlx::query_as::<_, (String, Option<uuid::Uuid>)>(
        "SELECT branch, sync_attempt_id FROM flakes WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(flake_id)
    .fetch_optional(&mut *tx)
    .await;

    match current_state {
        Ok(Some((cur_branch, cur_attempt))) => {
            if cur_branch != branch || cur_attempt != Some(attempt_id) {
                info!("Flake {flake_id} was reset or superseded before rewrite lock; aborting");
                let _ = tx.rollback().await;
                return Ok(HistoryRewriteOutcome::Superseded);
            }
        }
        Ok(None) => {
            let _ = tx.rollback().await;
            // Flake was deleted; record sync error so the 'syncing' marker
            // transitions to 'error' rather than being stuck (round 16).
            record_sync_error(
                pool,
                flake_id,
                attempt_id,
                repo_url,
                "Flake disappeared before rewrite mutation tx",
            )
            .await;
            bail!("Flake {flake_id} disappeared before rewrite mutation tx");
        }
        Err(e) => {
            error!("Recheck failed for rewrite (flake {flake_id}): {e:#}");
            let _ = tx.rollback().await;
            record_sync_error(pool, flake_id, attempt_id, repo_url, &e.to_string()).await;
            return Err(e.into());
        }
    }

    // ── Pre-filter existing hashes ──────────────────────────────────────
    // Query which of the snapshot hashes already exist in the commits table.
    // Only genuinely new hashes are inserted below.
    let candidate_hashes: Vec<&str> = commits.iter().map(|c| c.hash.as_str()).collect();
    let existing: std::collections::HashSet<String> = if candidate_hashes.is_empty() {
        std::collections::HashSet::new()
    } else {
        match sqlx::query_scalar::<_, String>(
            "SELECT git_commit_hash FROM commits WHERE flake_id = $1 AND git_commit_hash = ANY($2)",
        )
        .bind(flake_id)
        .bind(&candidate_hashes)
        .fetch_all(&mut *tx)
        .await
        {
            Ok(rows) => rows.into_iter().collect(),
            Err(e) => {
                error!("Pre-filter query failed during rewrite (flake {flake_id}): {e:#}");
                let _ = tx.rollback().await;
                record_sync_error(pool, flake_id, attempt_id, repo_url, &e.to_string()).await;
                return Err(e.into());
            }
        }
    };

    // ── Insert only genuinely new hashes ────────────────────────────────
    let mut inserted_count: u64 = 0;
    for commit_data in &commits {
        if existing.contains(&commit_data.hash) {
            continue;
        }
        match insert_commit_by_flake_id_tx(
            &mut tx,
            flake_id,
            &commit_data.hash,
            commit_data.timestamp,
            Some(&commit_data.message),
            Some(&commit_data.author),
        )
        .await
        {
            Ok(n) => inserted_count += n,
            Err(e) => {
                error!(
                    "Insert failed for {} during rewrite (flake {flake_id}): {e:#}",
                    commit_data.hash
                );
                let _ = tx.rollback().await;
                record_sync_error(pool, flake_id, attempt_id, repo_url, &e.to_string()).await;
                return Err(e);
            }
        }
    }

    // ── Resolve ordered IDs and replace snapshot ────────────────────────
    let resolved = match resolve_ordered_ids_tx(&mut tx, flake_id, &ordered_hashes).await {
        Ok(ids) => ids,
        Err(e) => {
            error!("ID resolution failed during rewrite (flake {flake_id}): {e:#}");
            let _ = tx.rollback().await;
            record_sync_error(pool, flake_id, attempt_id, repo_url, &e.to_string()).await;
            return Err(e);
        }
    };

    // Capture the current snapshot IDs before replacement so we can tell
    // which commits left the active snapshot.  The archived commit rows are
    // preserved, but eval/build attention for commits no longer in the
    // active snapshot should be resolved (round 16 review).
    let previous_snapshot_ids: Vec<i32> = match sqlx::query_scalar(
        "SELECT commit_id FROM flake_branch_commit_snapshot WHERE flake_id = $1",
    )
    .bind(flake_id)
    .fetch_all(&mut *tx)
    .await
    {
        Ok(ids) => ids,
        Err(e) => {
            error!("Failed to read previous snapshot IDs for flake {flake_id}: {e:#}");
            let _ = tx.rollback().await;
            record_sync_error(pool, flake_id, attempt_id, repo_url, &e.to_string()).await;
            return Err(e.into());
        }
    };

    if let Err(e) =
        crate::queries::flakes::replace_flake_branch_snapshot(&mut tx, flake_id, &resolved).await
    {
        error!("Snapshot publication failed during rewrite (flake {flake_id}): {e:#}");
        let _ = tx.rollback().await;
        record_sync_error(pool, flake_id, attempt_id, repo_url, &e.to_string()).await;
        return Err(e);
    }

    // Resolve eval/build attention for commits that left the active snapshot.
    let current_snapshot_ids: std::collections::HashSet<i32> =
        resolved.iter().copied().collect();
    let removed_from_snapshot: Vec<i32> = previous_snapshot_ids
        .into_iter()
        .filter(|id| !current_snapshot_ids.contains(id))
        .collect();

    if !removed_from_snapshot.is_empty() {
        let removed_commit_subjects: Vec<String> =
            removed_from_snapshot.iter().map(ToString::to_string).collect();
        let removed_build_subjects: Vec<String> = match sqlx::query_scalar(
            r#"
            SELECT bj.id::text
            FROM build_jobs bj
            JOIN derivations d ON d.id = bj.derivation_id
            WHERE d.commit_id = ANY($1)
            "#,
        )
        .bind(&removed_from_snapshot)
        .fetch_all(&mut *tx)
        .await
        {
            Ok(ids) => ids,
            Err(e) => {
                error!(
                    "Failed to read removed build job IDs for flake {flake_id}: {e:#}"
                );
                let _ = tx.rollback().await;
                record_sync_error(pool, flake_id, attempt_id, repo_url, &e.to_string()).await;
                return Err(e.into());
            }
        };

        if let Err(e) = sqlx::query(
            r#"
            UPDATE attention_occurrences
            SET resolved_at = statement_timestamp()
            WHERE resolved_at IS NULL
              AND (
                  (category = 'evals' AND subject_id = ANY($1::text[]))
                  OR
                  (category = 'builds' AND subject_id = ANY($2::text[]))
              )
            "#,
        )
        .bind(&removed_commit_subjects)
        .bind(&removed_build_subjects)
        .execute(&mut *tx)
        .await
        {
            error!(
                "Failed to resolve attention for removed snapshot commits on flake {flake_id}: {e:#}"
            );
            let _ = tx.rollback().await;
            record_sync_error(pool, flake_id, attempt_id, repo_url, &e.to_string()).await;
            return Err(e.into());
        }
    }

    // ── Acquire attention lock and resolve flake-level attention ────────
    let attention_lock_key = format!("attention_occurrence:flakes:{flake_id}");
    if let Err(e) = sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(&attention_lock_key)
        .execute(&mut *tx)
        .await
    {
        error!("Failed to acquire attention lock during rewrite (flake {flake_id}): {e:#}");
        let _ = tx.rollback().await;
        record_sync_error(pool, flake_id, attempt_id, repo_url, &e.to_string()).await;
        return Err(e.into());
    }

    // Update sync status.
    let status_update = sqlx::query(
        "UPDATE flakes \
         SET sync_status = 'synced', last_sync_at = now(), last_sync_error = NULL, last_synced_at = now() \
         WHERE id = $1 AND deleted_at IS NULL AND sync_attempt_id = $2",
    )
    .bind(flake_id)
    .bind(attempt_id)
    .execute(&mut *tx)
    .await;

    match status_update {
        Ok(upd) if upd.rows_affected() == 1 => {}
        Ok(_) => {
            info!("Flake {flake_id} rewrite sync was superseded before status update; aborting");
            let _ = tx.rollback().await;
            // The initial syncing marker plus the git work are discarded by
            // the rollback; record_sync_error is not called because the
            // superseding sync will publish its own status.
            return Ok(HistoryRewriteOutcome::Superseded);
        }
        Err(e) => {
            error!("Status update failed during rewrite (flake {flake_id}): {e:#}");
            let _ = tx.rollback().await;
            record_sync_error(pool, flake_id, attempt_id, repo_url, &e.to_string()).await;
            return Err(e.into());
        }
    }

    // Resolve flake attention.
    if let Err(e) = sqlx::query(
        "UPDATE attention_occurrences SET resolved_at = NOW() \
         WHERE category = 'flakes' AND subject_id = $1 AND resolved_at IS NULL",
    )
    .bind(flake_id.to_string())
    .execute(&mut *tx)
    .await
    {
        error!("Failed to resolve flake attention during rewrite (flake {flake_id}): {e:#}");
        let _ = tx.rollback().await;
        record_sync_error(pool, flake_id, attempt_id, repo_url, &e.to_string()).await;
        return Err(e.into());
    }

    // Commit everything atomically.
    if let Err(e) = tx.commit().await {
        error!("Rewrite commit failed (flake {flake_id}): {e:#}");
        record_sync_error(pool, flake_id, attempt_id, repo_url, &e.to_string()).await;
        return Err(e.into());
    }

    if inserted_count > 0 {
        info!(
            "✅ Rewrite accepted for flake {flake_id}: inserted {} new commits ({} in snapshot)",
            inserted_count,
            ordered_hashes.len()
        );
    }

    Ok(HistoryRewriteOutcome::Applied {
        deleted_commits: 0,
        inserted_commits: inserted_count,
    })
}

/// Maximum number of commits to retain in the branch-commit snapshot.
///
/// Must be at least the maximum timeline limit (500) so the snapshot can
/// satisfy every supported Flakes timeline request without git operations.
const MAX_SNAPSHOT_COMMITS: i64 = 500;

/// Record a sync error on the flake row (best-effort, errors logged only).
///
/// This is the single consolidated error-recording path for every failure
/// mode in `sync_flake_recorded` (git fetch failure, lock failure, insert
/// failure, snapshot failure, status-update failure, commit failure), so it
/// is also the single place that opens/re-observes the flake's attention
/// occurrence for a sync error.
async fn record_sync_error(
    pool: &PgPool,
    flake_id: i32,
    attempt_id: Uuid,
    repo_url: &str,
    error_text: &str,
) {
    let truncated = sanitize_and_truncate_sync_error(repo_url, error_text, 4000);
    let update_result = sqlx::query(
        "UPDATE flakes \
         SET sync_status = 'error', last_sync_at = now(), last_sync_error = $2 \
         WHERE id = $1 AND deleted_at IS NULL AND sync_attempt_id = $3",
    )
    .bind(flake_id)
    .bind(&truncated)
    .bind(attempt_id)
    .execute(pool)
    .await;

    match update_result {
        Ok(update) if update.rows_affected() == 0 => {
            // Superseded by a newer attempt — do not open an attention
            // occurrence on behalf of an attempt that is no longer current.
        }
        Ok(_) => {
            let metadata = serde_json::json!({
                "flake_id": flake_id,
                "last_sync_error": &truncated,
            });
            // Open the sync_error occurrence, but only if this attempt is
            // still the current, errored result recorded on the flake row.
            // A delayed caller here could otherwise open a stale sync_error
            // occurrence after a NEWER attempt has already succeeded (and
            // resolved attention) — see
            // transition_flake_attention_to_error_if_current.
            transition_flake_attention_to_error_if_current(pool, flake_id, attempt_id, metadata)
                .await;
        }
        Err(e) => {
            warn!("Failed to record sync error for flake {flake_id}: {e:#}");
        }
    }
}

/// Resolve flake attention occurrences, but only if `attempt_id` is still
/// the current, synced attempt recorded on the flake row.
///
/// The status commit (`sync_status = 'synced'`) and this attention action
/// are two separate operations, so a delay between them (e.g. this async
/// call is scheduled late) leaves a window in which a NEWER sync attempt
/// can start and fail, opening a `sync_error` occurrence. Without this
/// recheck, this (now-stale) success handler would resolve that newer
/// attempt's `sync_error` occurrence — even though the flake's actual
/// current state is `error`, not `synced`.
///
/// Acquires the same per-subject advisory lock used by
/// [`transition_flake_attention_to_error_if_current`] and the stale-flake
/// reconciler, so the recheck-then-act sequence is atomic with respect to
/// any concurrent attention transition for this flake.
///
/// `pub(crate)` so the periodic reconciliation sweep
/// (`tasks::attention_reconciliation::reconcile_synced_flakes_missing_resolution`)
/// can invoke it as a safety net for flakes whose successful sync commit
/// succeeded but whose attention resolution was lost to a process crash or
/// transient failure between the two (separate, best-effort) operations.
pub(crate) async fn resolve_flake_attention_if_current(
    pool: &PgPool,
    flake_id: i32,
    attempt_id: Uuid,
) {
    let subject_id = flake_id.to_string();

    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(e) => {
            warn!("failed to begin flake attention resolve transaction: {e:#}");
            return;
        }
    };

    let lock_key = format!("attention_occurrence:flakes:{subject_id}");
    if let Err(e) = sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(&lock_key)
        .execute(&mut *tx)
        .await
    {
        warn!("failed to acquire flake attention lock: {e:#}");
        let _ = tx.rollback().await;
        return;
    }

    // Recheck under the lock: is this attempt still the current, synced
    // result? If a newer attempt has since started or failed, do nothing —
    // that newer attempt owns the flake's attention state now.
    //
    // `deleted_at IS NULL` is required here (round 11): without it, a
    // delayed call racing a soft/hard delete could find the DELETED row's
    // stale `sync_attempt_id`/`sync_status` still matching (soft delete does
    // not clear either) and resolve occurrences for a flake that the delete
    // path already resolved and intentionally closed out — harmless on its
    // own here since resolve is idempotent, but the same predicate is
    // required in `transition_flake_attention_to_error_if_current` where the
    // consequence is a newly INSERTED occurrence for a deleted flake, so
    // both recheck queries use the identical guard for consistency.
    let still_current: bool = match sqlx::query_scalar(
        "SELECT sync_attempt_id = $2 AND sync_status = 'synced' FROM flakes \
         WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(flake_id)
    .bind(attempt_id)
    .fetch_optional(&mut *tx)
    .await
    {
        Ok(Some(v)) => v,
        Ok(None) => false,
        Err(e) => {
            warn!("failed to recheck flake state before resolving attention: {e:#}");
            let _ = tx.rollback().await;
            return;
        }
    };

    if !still_current {
        // Superseded — commit the (no-op) transaction to release the lock.
        let _ = tx.commit().await;
        return;
    }

    if let Err(e) = sqlx::query(
        "UPDATE attention_occurrences SET resolved_at = NOW() \
         WHERE category = 'flakes' AND subject_id = $1 AND resolved_at IS NULL",
    )
    .bind(&subject_id)
    .execute(&mut *tx)
    .await
    {
        warn!("failed to resolve flake attention occurrence: {e:#}");
        let _ = tx.rollback().await;
        return;
    }

    if let Err(e) = tx.commit().await {
        warn!("failed to commit flake attention resolve: {e:#}");
    }
}

/// Transition flake attention to `sync_error`, but only if `attempt_id` is
/// still the current, errored attempt recorded on the flake row.
///
/// Mirrors [`resolve_flake_attention_if_current`]'s reasoning for the
/// opposite direction: the status commit (`sync_status = 'error'`) and this
/// attention action are separate operations, so a delay here could open a
/// stale `sync_error` occurrence after a NEWER attempt has already
/// succeeded and resolved attention. Acquires the same per-subject
/// advisory lock, so this recheck-then-act sequence is atomic with respect
/// to any concurrent attention transition for this flake.
///
/// Beyond that direct race, an existing `sync_error` occurrence found under
/// the lock is only ever *observed* (reused) if no successful sync has
/// completed since it was last observed — checked via `flakes.last_synced_at`,
/// which (unlike `last_sync_at`) is never clobbered by a later failure. Without
/// this check: attempt A fails and opens/observes occurrence O (user dismisses
/// it); attempt B succeeds but its `resolve_flake_attention_if_current` call
/// is lost to a crash, so O is never resolved; attempt C later fails and would
/// find O still open with a matching reason and simply observe it — silently
/// inheriting A's original `opened_at` and the user's dismissal for what is,
/// from the user's point of view, a brand new failure they have never seen.
/// When staleness is detected, O is resolved and a fresh occurrence is opened
/// instead, exactly as if no prior occurrence existed.
///
/// `pub(crate)` so the periodic reconciliation sweep
/// (`tasks::attention_reconciliation::reconcile_errored_flakes`) can invoke
/// it as a safety net for flakes whose `sync_status = 'error'` commit
/// succeeded but whose attention transition was lost to a process crash or
/// transient failure between the two (separate, best-effort) operations.
pub(crate) async fn transition_flake_attention_to_error_if_current(
    pool: &PgPool,
    flake_id: i32,
    attempt_id: Uuid,
    mut metadata: serde_json::Value,
) {
    let subject_id = flake_id.to_string();

    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(e) => {
            warn!("failed to begin flake attention transition transaction: {e:#}");
            return;
        }
    };

    let lock_key = format!("attention_occurrence:flakes:{subject_id}");
    if let Err(e) = sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(&lock_key)
        .execute(&mut *tx)
        .await
    {
        warn!("failed to acquire flake attention lock: {e:#}");
        let _ = tx.rollback().await;
        return;
    }

    // Recheck under the lock: is this attempt still the current, errored
    // result? If a newer attempt has since started or succeeded, do
    // nothing — that newer attempt owns the flake's attention state now.
    //
    // Two DISTINCT timestamps are fetched, not one:
    //   - `opened_at` (from `last_sync_at`): the authoritative time
    //     `record_sync_error` recorded THIS failure. Used for the
    //     occurrence's `opened_at` (and, on a fresh insert, its initial
    //     `last_observed_at`) so a recovered historical failure does not
    //     get a brand-new 24-hour attention window dated "now".
    //   - `observed_at` (`statement_timestamp()`, i.e. after the lock wait,
    //     not `NOW()`/`transaction_timestamp()` which are fixed at
    //     transaction start): used to bump `last_observed_at` when
    //     observing an EXISTING occurrence, so repeated reconciliation
    //     sweeps against a still-ongoing historical incident keep
    //     `last_observed_at` advancing instead of being pinned to the
    //     original (possibly days-old) failure time forever.
    // `last_synced_at` is also fetched for the staleness check described
    // above.
    //
    // `deleted_at IS NULL` is required here (round 11): without it, a
    // delayed error transition (e.g. `record_sync_error` scheduled late
    // relative to a concurrent soft/hard delete of the same flake) could
    // find the deleted row's stale `sync_attempt_id = error` still matching
    // and INSERT a brand new attention occurrence for a flake the user can
    // no longer see or dismiss — resurrecting a badge for something already
    // deleted. When the row is filtered out, this query returns `None`,
    // which the match below already treats as "not current" (no-op).
    let recheck: Option<(bool, DateTime<Utc>, DateTime<Utc>, Option<DateTime<Utc>>)> =
        match sqlx::query_as(
            "SELECT sync_attempt_id = $2 AND sync_status = 'error', \
                    COALESCE(last_sync_at, statement_timestamp()), statement_timestamp(), last_synced_at \
             FROM flakes WHERE id = $1 AND deleted_at IS NULL",
        )
        .bind(flake_id)
        .bind(attempt_id)
        .fetch_optional(&mut *tx)
        .await
        {
            Ok(v) => v,
            Err(e) => {
                warn!("failed to recheck flake state before opening attention: {e:#}");
                let _ = tx.rollback().await;
                return;
            }
        };

    let (still_current, opened_at, observed_at, last_synced_at) = match recheck {
        Some((current, opened, observed, synced)) => (current, opened, observed, synced),
        None => (false, Utc::now(), Utc::now(), None),
    };

    if !still_current {
        let _ = tx.commit().await;
        return;
    }

    if let serde_json::Value::Object(ref mut map) = metadata {
        map.insert(
            "reason".to_string(),
            serde_json::Value::String("sync_error".to_string()),
        );
        map.insert(
            "sync_attempt_id".to_string(),
            serde_json::Value::String(attempt_id.to_string()),
        );
    }

    // Check for an already-open occurrence with the same reason, also
    // fetching its last_observed_at for the staleness check below.
    let existing: Option<(Uuid, DateTime<Utc>)> = match sqlx::query_as(
        r#"
        SELECT id, last_observed_at FROM attention_occurrences
        WHERE category = 'flakes'
          AND subject_id = $1
          AND resolved_at IS NULL
          AND metadata @> $2::jsonb
        LIMIT 1
        FOR UPDATE
        "#,
    )
    .bind(&subject_id)
    .bind(serde_json::json!({"reason": "sync_error"}))
    .fetch_optional(&mut *tx)
    .await
    {
        Ok(v) => v,
        Err(e) => {
            warn!("failed to find existing flake occurrence: {e:#}");
            let _ = tx.rollback().await;
            return;
        }
    };

    // An existing occurrence is only reusable if no successful sync has
    // completed since it was last observed. Otherwise it belongs to an
    // earlier, now-superseded incident and must not be reused for this one.
    let existing = existing.filter(|(_, last_observed_at)| {
        !matches!(last_synced_at, Some(synced) if synced > *last_observed_at)
    });

    if let Some((id, _)) = existing {
        if let Err(e) = sqlx::query(
            "UPDATE attention_occurrences \
             SET metadata = CASE WHEN $1 >= last_observed_at THEN $2 ELSE metadata END, \
                 last_observed_at = GREATEST(last_observed_at, $1) \
             WHERE id = $3",
        )
        .bind(observed_at)
        .bind(&metadata)
        .bind(id)
        .execute(&mut *tx)
        .await
        {
            warn!("failed to update flake attention occurrence: {e:#}");
            let _ = tx.rollback().await;
            return;
        }
    } else {
        // Reason differs, occurrence is stale (superseded by an intervening
        // success), or none exists — resolve all open occurrences (e.g. a
        // stale_sync from the reconciler, or a stale sync_error from a
        // resolved-then-recurred incident) and insert a new sync_error
        // occurrence.
        if let Err(e) = sqlx::query(
            "UPDATE attention_occurrences SET resolved_at = NOW() \
             WHERE category = 'flakes' AND subject_id = $1 AND resolved_at IS NULL",
        )
        .bind(&subject_id)
        .execute(&mut *tx)
        .await
        {
            warn!("failed to resolve open flake occurrences: {e:#}");
            let _ = tx.rollback().await;
            return;
        }

        let episode_id = Uuid::new_v4();
        let source_key = attention::flake_occurrence_key(flake_id, episode_id);

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
        .bind(observed_at)
        .bind(&metadata)
        .execute(&mut *tx)
        .await
        {
            warn!("failed to insert flake attention occurrence: {e:#}");
            let _ = tx.rollback().await;
            return;
        }
    }

    if let Err(e) = tx.commit().await {
        warn!("failed to commit flake attention transition: {e:#}");
    }
}

/// Resolve ordered git hashes to contiguous database commit IDs.
///
/// Takes hashes in Git traversal order (HEAD first, from the sync's own git
/// log), queries the DB for matching (hash, id) pairs, and builds a contiguous
/// prefix starting from HEAD:
///
///   - HEAD (index 0) MUST resolve to a DB ID. If it does not, returns `Err`
///     and the sync is treated as failed.
///   - Iterates forward until a hash does not resolve. The prefix up to (but
///     not including) that unresolvable hash is returned.  This prevents gaps
///     in `commits_behind`.
///   - An empty slice is valid (empty Git log = empty ready snapshot).
async fn resolve_ordered_ids(
    pool: &PgPool,
    flake_id: i32,
    ordered_hashes: &[String],
) -> Result<Vec<i32>> {
    if ordered_hashes.is_empty() {
        return Ok(Vec::new());
    }

    let hash_slice: Vec<&str> = ordered_hashes.iter().map(|h| h.as_str()).collect();
    let rows: Vec<(String, i32)> = sqlx::query_as(
        r#"
        SELECT git_commit_hash, id
        FROM commits
        WHERE flake_id = $1
          AND git_commit_hash = ANY($2)
        "#,
    )
    .bind(flake_id)
    .bind(&hash_slice)
    .fetch_all(pool)
    .await
    .context("Failed to resolve commit hashes for snapshot")?;

    let map: std::collections::HashMap<&str, i32> =
        rows.iter().map(|(h, id)| (h.as_str(), *id)).collect();

    // HEAD (index 0) MUST resolve.
    let head = &ordered_hashes[0];
    if !map.contains_key(head.as_str()) {
        bail!(
            "HEAD commit {} was not inserted into the database after sync (flake {})",
            head,
            flake_id
        );
    }

    // Build contiguous prefix: iterate from HEAD forward, stop at first gap.
    let mut ids = Vec::with_capacity(ordered_hashes.len());
    for hash in ordered_hashes {
        match map.get(hash.as_str()) {
            Some(id) => ids.push(*id),
            None => break,
        }
    }

    Ok(ids)
}

/// Like `resolve_ordered_ids` but runs inside an open transaction so newly
/// inserted commits are visible without waiting for a commit.
async fn resolve_ordered_ids_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    flake_id: i32,
    ordered_hashes: &[String],
) -> Result<Vec<i32>> {
    if ordered_hashes.is_empty() {
        return Ok(Vec::new());
    }

    let hash_slice: Vec<&str> = ordered_hashes.iter().map(|h| h.as_str()).collect();
    let rows: Vec<(String, i32)> = sqlx::query_as(
        r#"
        SELECT git_commit_hash, id
        FROM commits
        WHERE flake_id = $1
          AND git_commit_hash = ANY($2)
        "#,
    )
    .bind(flake_id)
    .bind(&hash_slice)
    .fetch_all(&mut **tx)
    .await
    .context("Failed to resolve commit hashes for snapshot")?;

    let map: std::collections::HashMap<&str, i32> =
        rows.iter().map(|(h, id)| (h.as_str(), *id)).collect();

    let head = &ordered_hashes[0];
    if !map.contains_key(head.as_str()) {
        bail!(
            "HEAD commit {} was not inserted into the database after sync (flake {})",
            head,
            flake_id
        );
    }

    let mut ids = Vec::with_capacity(ordered_hashes.len());
    for hash in ordered_hashes {
        match map.get(hash.as_str()) {
            Some(id) => ids.push(*id),
            None => break,
        }
    }

    Ok(ids)
}

fn sanitize_and_truncate_sync_error(repo_url: &str, raw: &str, max_chars: usize) -> String {
    let sanitized_repo = redact_url_credentials(repo_url);
    let replaced_repo = raw.replace(repo_url, &sanitized_repo);
    let redacted = redact_sensitive_tokens(&replaced_repo);
    truncate_error_text(&redacted, max_chars)
}

fn truncate_error_text(input: &str, max_chars: usize) -> String {
    let mut chars = input.chars();
    let mut out = String::new();
    for _ in 0..max_chars {
        if let Some(ch) = chars.next() {
            out.push(ch);
        } else {
            return out;
        }
    }
    if chars.next().is_some() {
        out.push('…');
    }
    out
}

fn redact_sensitive_tokens(input: &str) -> String {
    let mut out = String::new();
    let mut idx = 0;
    while idx < input.len() {
        let remaining = &input[idx..];
        let remaining_lower = remaining.to_ascii_lowercase();
        if remaining_lower.starts_with("authorization:") {
            out.push_str("Authorization: [REDACTED]");
            if let Some(pos) = remaining.find('\n') {
                idx += pos;
            } else {
                break;
            }
            continue;
        }
        if remaining.starts_with("http://") || remaining.starts_with("https://") {
            let end = remaining
                .find(|c: char| c.is_whitespace() || matches!(c, '\'' | '"' | ')' | ']' | '>'))
                .unwrap_or(remaining.len());
            let candidate = &remaining[..end];
            out.push_str(&redact_url_credentials(candidate));
            idx += end;
            continue;
        }
        if remaining_lower.starts_with("git_askpass=") {
            out.push_str("GIT_ASKPASS=[REDACTED]");
            if let Some(pos) = remaining.find(char::is_whitespace) {
                idx += pos;
            } else {
                break;
            }
            continue;
        }
        if remaining_lower.starts_with("netrc=") || remaining_lower.starts_with("netrc_file=") {
            let key = if remaining_lower.starts_with("netrc_file=") {
                "NETRC_FILE"
            } else {
                "NETRC"
            };
            out.push_str(key);
            out.push_str("=[REDACTED]");
            if let Some(pos) = remaining.find(char::is_whitespace) {
                idx += pos;
            } else {
                break;
            }
            continue;
        }
        let ch = remaining.chars().next().unwrap();
        out.push(ch);
        idx += ch.len_utf8();
    }
    out
}

fn redact_url_credentials(input: &str) -> String {
    let Ok(mut url) = Url::parse(input) else {
        return input.to_string();
    };
    url.set_query(None);
    url.set_fragment(None);
    let has_auth = !url.username().is_empty() || url.password().is_some();
    if !has_auth {
        return url.to_string();
    }
    let _ = url.set_username("REDACTED");
    let _ = url.set_password(None);
    url.to_string()
}

/// Fetch commits from the remote and run force-push detection.  Does NOT insert.
///
/// Returns the commits in Git log order (HEAD first) so the caller can insert
/// them inside a guarded transaction that holds the per-flake advisory lock.
async fn collect_git_commits(
    pool: &PgPool,
    flake_id: i32,
    repo_url: &str,
    branch: &str,
    creds: Option<&FlakeCredentialEnv>,
) -> Result<Vec<CommitData>> {
    let max = MAX_SNAPSHOT_COMMITS as usize;
    let (commits, clone_dir) =
        get_commits_with_full_metadata_and_dir(repo_url, branch, Some(max), None, creds)
            .await
            .with_context(|| format!("Failed to list commits for {repo_url} on {branch}"))?;
    let clone_path = clone_dir.path().to_owned();
    let ordered_hashes: Vec<String> = commits.iter().map(|c| c.hash.clone()).collect();

    // Force-push detection using the authoritative snapshot HEAD (position 0),
    // not the timestamp-max commit.  DB error propagates as sync failure.
    let previous_head: Option<String> = sqlx::query_scalar(
        r#"
        SELECT c.git_commit_hash
        FROM flake_branch_commit_snapshot fbcs
        JOIN commits c ON c.id = fbcs.commit_id
        WHERE fbcs.flake_id = $1
          AND fbcs.position = 0
        "#,
    )
    .bind(flake_id)
    .fetch_optional(pool)
    .await
    .context("Failed to read previous snapshot HEAD")?;

    if let Some(ref prev) = previous_head {
        if let Some(ref head) = ordered_hashes.first() {
            if *head != prev && !ordered_hashes.contains(prev) {
                let is_ancestor =
                    check_git_ancestry_in_clone(&clone_path, prev, head, branch, creds).await?;
                if !is_ancestor {
                    warn!(
                        repo_url = %repo_url,
                        branch = %branch,
                        previous_head = %prev,
                        remote_head = %head,
                        "history_rewrite_detected via missing snapshot HEAD"
                    );
                    return Err(anyhow::anyhow!(
                        "{}: remote history diverged for {} on {}. \
                         Previous HEAD {} is no longer in branch history. \
                         Accept rewrite via POST /api/v1/flakes/:id/accept-rewrite \
                         before syncing again. Remote HEAD is {}.",
                        HISTORY_REWRITE_ERROR_MARKER,
                        repo_url,
                        branch,
                        prev,
                        head,
                    ));
                }
            }
        }
    }

    // Keep the TempDir alive until we're done with force-push detection.
    drop(clone_dir);
    Ok(commits)
}

/// Run a full sync for a repository and return (inserted_count, ordered_hashes).
///
/// The ordered hashes are from `git log --max-count=MAX_SNAPSHOT_COMMITS` in Git
/// traversal order (HEAD first). This is the SAME Git observation used for
/// insertion — no second clone is performed for snapshot purposes.
///
/// Force-push detection: the last known commit hash is checked against the git
/// log results. If the remote HEAD has changed and the last known commit is not
/// in the git log, a `HISTORY_REWRITE_ERROR_MARKER` error is returned.
async fn sync_commits_for_repo_inner(
    pool: &PgPool,
    repo_url: &str,
    branch: &str,
    creds: Option<&FlakeCredentialEnv>,
) -> Result<(u64, Vec<String>)> {
    let max = MAX_SNAPSHOT_COMMITS as usize;
    // Retain the temp_dir so the clone stays alive for ancestry verification.
    let (commits, _clone_dir) =
        get_commits_with_full_metadata_and_dir(repo_url, branch, Some(max), None, creds)
            .await
            .with_context(|| format!("Failed to list commits for {repo_url} on {branch}"))?;
    let clone_path = _clone_dir.path().to_owned();

    let ordered_hashes: Vec<String> = commits.iter().map(|c| c.hash.clone()).collect();

    // Force-push detection: read the previous branch HEAD from the snapshot
    // (position 0). If it differs from the current Git log HEAD and is not
    // in the log, the branch was rewritten.  Using snapshot position 0 is
    // authoritative — it is the previous Git HEAD, not the timestamp-max
    // commit.
    let previous_head: Option<String> = sqlx::query_scalar(
        r#"
        SELECT c.git_commit_hash
        FROM flake_branch_commit_snapshot fbcs
        JOIN commits c ON c.id = fbcs.commit_id
        WHERE fbcs.flake_id = (SELECT id FROM flakes WHERE repo_url = $1)
          AND fbcs.position = 0
        "#,
    )
    .bind(repo_url)
    .fetch_optional(pool)
    .await
    .context("Failed to read previous snapshot HEAD")?;

    if let Some(ref prev) = previous_head {
        let current_head = ordered_hashes.first().cloned();
        if let Some(ref head) = current_head {
            if head != prev && !ordered_hashes.contains(prev) {
                // The previous HEAD is outside the MAX_SNAPSHOT_COMMITS window.
                // Before declaring a force-push, deepen the clone and verify
                // ancestry — the branch may have advanced by 500+ commits.
                // Verify ancestry against the exact observed HEAD from the
                // first clone — not a second clone's potentially different HEAD.
                let observed = ordered_hashes.first().cloned();
                // Propagate ancestry verification errors as sync failures so
                // transient failures do not appear as history rewrites.
                let is_ancestor = match observed {
                    Some(ref observed_head) => {
                        check_git_ancestry_in_clone(&clone_path, prev, observed_head, branch, creds)
                            .await?
                    }
                    None => false,
                };

                if !is_ancestor {
                    warn!(
                        repo_url = %repo_url,
                        branch = %branch,
                        previous_head = %prev,
                        remote_head = %head,
                        "history_rewrite_detected via missing snapshot HEAD"
                    );
                    return Err(anyhow::anyhow!(
                        "{}: remote history diverged for {} on {}. Previous HEAD {} is no longer in branch history. Accept rewrite via POST /api/v1/flakes/:id/accept-rewrite before syncing again. Remote HEAD is {}.",
                        HISTORY_REWRITE_ERROR_MARKER,
                        repo_url,
                        branch,
                        prev,
                        head,
                    ));
                }
            }
        }
    }

    // Pre-filter: query which of our candidate hashes already exist.
    // Querying only the candidates avoids scanning the entire commits table.
    let candidate_hashes: Vec<&str> = commits.iter().map(|c| c.hash.as_str()).collect();
    let existing: std::collections::HashSet<String> = if candidate_hashes.is_empty() {
        std::collections::HashSet::new()
    } else {
        let rows: Vec<String> = sqlx::query_scalar(
            r#"
            SELECT git_commit_hash
            FROM commits
            WHERE flake_id = (SELECT id FROM flakes WHERE repo_url = $1)
              AND git_commit_hash = ANY($2)
            "#,
        )
        .bind(repo_url)
        .bind(&candidate_hashes)
        .fetch_all(pool)
        .await
        .context("Failed to query existing commit hashes")?;
        rows.into_iter().collect()
    };

    let mut inserted_count: u64 = 0;
    for commit_data in &commits {
        if existing.contains(&commit_data.hash) {
            continue;
        }
        match insert_commit_with_metadata(
            pool,
            &commit_data.hash,
            repo_url,
            commit_data.timestamp,
            Some(&commit_data.message),
            Some(&commit_data.author),
        )
        .await
        {
            Ok(n) => inserted_count += n,
            Err(e) => warn!("Failed to insert commit {}: {}", commit_data.hash, e),
        }
    }

    if inserted_count > 0 {
        info!(
            "✅ Synced {} ({}): {} new commits ({} in git log)",
            repo_url,
            branch,
            inserted_count,
            ordered_hashes.len()
        );
    }

    Ok((inserted_count, ordered_hashes))
}

/// Check whether `ancestor_hash` is an ancestor of `observed_head` using the
/// SAME clone directory that produced the git log.  This avoids a second clone
/// and ensures both hashes come from the same Git observation.
///
/// Deepens the existing shallow clone (with credentials) until either:
///   - the ancestor commit becomes reachable (proceed to merge-base), or
///   - the repository is no longer shallow (`git rev-parse
///     --is-shallow-repository` returns false) and the ancestor is still
///     absent — this is conclusive proof of a genuine history rewrite.
///
/// A fixed iteration cap is NOT used to distinguish rewrite from incomplete
/// history: `git fetch --deepen` naturally stops adding commits once the
/// repository root is reached, at which point the repo becomes non-shallow.
///
/// Exit code 0 from merge-base = is ancestor; 1 = not an ancestor (rewrite);
/// any other outcome (including deepen/rev-parse failures) propagates as Err
/// so transient Git failures are never misreported as a history rewrite.
async fn check_git_ancestry_in_clone(
    clone_path: &std::path::Path,
    ancestor_hash: &str,
    observed_head: &str,
    branch: &str,
    creds: Option<&FlakeCredentialEnv>,
) -> Result<bool> {
    fn commit_exists(clone_path: &std::path::Path, hash: &str) -> Result<bool> {
        let out = std::process::Command::new("git")
            .args(["cat-file", "-t", hash])
            .current_dir(clone_path)
            .output()
            .map_err(|e| anyhow::anyhow!("Failed to check commit existence: {e}"))?;
        Ok(out.status.success())
    }

    fn is_shallow(clone_path: &std::path::Path) -> Result<bool> {
        let out = std::process::Command::new("git")
            .args(["rev-parse", "--is-shallow-repository"])
            .current_dir(clone_path)
            .output()
            .map_err(|e| anyhow::anyhow!("Failed to check shallow state: {e}"))?;
        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr);
            bail!(
                "git rev-parse --is-shallow-repository failed: {}",
                stderr.trim()
            );
        }
        Ok(String::from_utf8_lossy(&out.stdout).trim() == "true")
    }

    while !commit_exists(clone_path, ancestor_hash)? {
        if !is_shallow(clone_path)? {
            // Full history has been fetched and the ancestor is still absent.
            // This is conclusive: the commit was genuinely removed from the
            // branch's reachable history — a real force-push/rewrite.
            return Ok(false);
        }

        let mut deepen = tokio::process::Command::new("git");
        deepen.args(["fetch", "--deepen=5000", "origin", branch]);
        apply_optional_creds(&mut deepen, creds);
        deepen.current_dir(clone_path);
        let out = deepen
            .output()
            .await
            .with_context(|| "Failed to deepen clone for ancestry check")?;
        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr);
            bail!("Git deepen failed during ancestry check: {}", stderr.trim());
        }
    }

    let mut mb = tokio::process::Command::new("git");
    mb.args(["merge-base", "--is-ancestor", ancestor_hash, observed_head]);
    mb.current_dir(clone_path);
    let out = mb
        .output()
        .await
        .with_context(|| "Failed to spawn git merge-base for ancestry check")?;

    match out.status.code() {
        Some(0) => Ok(true),  // is ancestor — normal fast-forward
        Some(1) => Ok(false), // not an ancestor — rewrite
        code => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            bail!(
                "git merge-base exited with unexpected code {:?}: {}",
                code,
                stderr.trim()
            )
        }
    }
}

/// Resolve the remote default branch name for a repository.
pub async fn infer_default_branch(repo_url: &str) -> Result<String> {
    infer_default_branch_with_creds(repo_url, None).await
}

pub async fn infer_default_branch_with_creds(
    repo_url: &str,
    creds: Option<&FlakeCredentialEnv>,
) -> Result<String> {
    let git_url = normalize_repo_url_for_git(repo_url, creds);
    let mut cmd = tokio::process::Command::new("git");
    cmd.args(["ls-remote", "--symref", &git_url, "HEAD"]);
    apply_optional_creds(&mut cmd, creds);
    let output = timeout(GIT_PROBE_TIMEOUT, cmd.output())
        .await
        .with_context(|| format!("Timed out probing default branch for {repo_url}"))?
        .with_context(|| format!("Failed to probe default branch for {repo_url}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("Git ls-remote failed for {repo_url}: {}", stderr.trim());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        if let Some(target) = line
            .strip_prefix("ref: refs/heads/")
            .and_then(|value| value.split('\t').next())
        {
            let branch = target.trim();
            if !branch.is_empty() {
                return Ok(branch.to_string());
            }
        }
    }

    bail!("Unable to determine default branch for {repo_url}")
}

/// Check whether a specific branch exists on the remote repository.
pub async fn branch_exists(repo_url: &str, branch: &str) -> Result<bool> {
    branch_exists_with_creds(repo_url, branch, None).await
}

pub async fn branch_exists_with_creds(
    repo_url: &str,
    branch: &str,
    creds: Option<&FlakeCredentialEnv>,
) -> Result<bool> {
    let git_url = normalize_repo_url_for_git(repo_url, creds);
    let refspec = format!("refs/heads/{branch}");

    let mut cmd = tokio::process::Command::new("git");
    cmd.args(["ls-remote", &git_url, &refspec]);
    apply_optional_creds(&mut cmd, creds);
    let output = timeout(GIT_PROBE_TIMEOUT, cmd.output())
        .await
        .with_context(|| format!("Timed out probing branch {branch} for {repo_url}"))?
        .with_context(|| format!("Failed to probe branch {branch} for {repo_url}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "Git ls-remote failed for {repo_url} on {branch}: {}",
            stderr.trim()
        );
    }

    Ok(!String::from_utf8_lossy(&output.stdout).trim().is_empty())
}

/// Apply optional flake credentials to a git command.
///
/// Injects netrc / SSH-key environment variables when `creds` is `Some`.
/// This is a thin pass-through so callers don't have to match on `Option`.
fn apply_optional_creds(cmd: &mut tokio::process::Command, creds: Option<&FlakeCredentialEnv>) {
    if let Some(c) = creds {
        c.apply_to_git_command(cmd);
    }
}

fn normalize_repo_url_for_git(repo_url: &str, creds: Option<&FlakeCredentialEnv>) -> String {
    let base_url = if let Some(stripped) = repo_url.strip_prefix("git+") {
        stripped
    } else if repo_url.starts_with("github:") {
        let repo_path = repo_url.strip_prefix("github:").unwrap();
        return format!("https://github.com/{}", repo_path);
    } else if repo_url.starts_with("gitlab:") {
        let repo_path = repo_url.strip_prefix("gitlab:").unwrap();
        return format!("https://gitlab.com/{}", repo_path);
    } else {
        repo_url
    };

    // Strip query parameters for git operations
    let normalized = if let Some(question_mark_pos) = base_url.find('?') {
        base_url[..question_mark_pos].to_string()
    } else {
        base_url.to_string()
    };

    if creds.map(|c| c.uses_ssh_key()).unwrap_or(false) {
        if let Some(converted) = normalize_https_hosted_git_to_ssh(&normalized) {
            return converted;
        }
    }

    normalized
}

fn normalize_https_hosted_git_to_ssh(url: &str) -> Option<String> {
    let trimmed = url.trim();
    let without_scheme = trimmed
        .strip_prefix("https://")
        .or_else(|| trimmed.strip_prefix("http://"))?;

    let (host, path) = without_scheme.split_once('/')?;
    if host.is_empty() || path.is_empty() {
        return None;
    }

    let path = path.trim_start_matches('/').trim_end_matches('/');
    if path.is_empty() {
        return None;
    }

    let path = if path.ends_with(".git") {
        path.to_string()
    } else {
        format!("{path}.git")
    };

    Some(format!("git@{host}:{path}"))
}

/// Get commits with timestamps, optionally since a specific commit
/// Commit data fetched from git log
#[derive(Debug, Clone)]
struct CommitData {
    hash: String,
    timestamp: chrono::DateTime<chrono::Utc>,
    message: String,
    author: String,
}

/// Like `get_commits_with_full_metadata` but also returns the temporary clone
/// directory so the caller can run additional git operations (e.g. ancestry
/// verification) on the SAME clone without creating a second network request.
async fn get_commits_with_full_metadata_and_dir(
    repo_url: &str,
    branch: &str,
    limit: Option<usize>,
    since_commit: Option<&str>,
    creds: Option<&FlakeCredentialEnv>,
) -> Result<(Vec<CommitData>, tempfile::TempDir)> {
    let git_url = normalize_repo_url_for_git(repo_url, creds);
    let temp_dir = tempfile::tempdir().context("Failed to create temporary directory")?;
    let clone_path = temp_dir.path();

    // Clone
    let depth = limit.unwrap_or(10).to_string();
    let mut clone_cmd = tokio::process::Command::new("git");
    clone_cmd
        .args(&[
            "clone",
            "--depth",
            &depth,
            "--branch",
            branch,
            "--single-branch",
            &git_url,
            ".",
        ])
        .current_dir(clone_path);
    apply_optional_creds(&mut clone_cmd, creds);
    let clone_output = clone_cmd.output().await?;

    if !clone_output.status.success() {
        let stderr = String::from_utf8_lossy(&clone_output.stderr);
        bail!("Git clone failed for {}: {}", repo_url, stderr);
    }

    // Build git log args with format: hash|timestamp|subject|author
    // Using %x1E as field separator (ASCII record separator) to handle multi-line messages
    let mut args = vec!["log", "--format=%H%x1E%cI%x1E%s%x1E%aN"];

    // Add range if since_commit provided
    let range;
    let max_count;

    if let Some(since) = since_commit {
        range = format!("{}..HEAD", since);
        args.push(&range);
    } else if let Some(lim) = limit {
        max_count = format!("--max-count={}", lim);
        args.push(&max_count);
    }

    let log_output = tokio::process::Command::new("git")
        .args(&args)
        .current_dir(clone_path)
        .output()
        .await
        .context("Failed to spawn git log")?;

    let mut log_output = log_output;

    if !log_output.status.success() {
        let stderr = String::from_utf8_lossy(&log_output.stderr);

        if since_commit.is_some() && stderr.contains("Invalid revision range") {
            let fetch_output = tokio::process::Command::new("git")
                .args(&["fetch", "--unshallow", "--tags", "origin", branch])
                .current_dir(clone_path)
                .output()
                .await
                .context("Failed to spawn git fetch --unshallow")?;

            if !fetch_output.status.success() {
                let fetch_stderr = String::from_utf8_lossy(&fetch_output.stderr);
                bail!("git fetch failed: {}", fetch_stderr.trim());
            }

            log_output = tokio::process::Command::new("git")
                .args(&args)
                .current_dir(clone_path)
                .output()
                .await
                .context("Failed to spawn git log (retry)")?;
        }
    }

    if !log_output.status.success() {
        let stderr = String::from_utf8_lossy(&log_output.stderr);
        bail!("git log failed: {}", stderr.trim());
    }

    let stdout = String::from_utf8(log_output.stdout)?;
    let commits: Result<Vec<_>> = stdout
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let parts: Vec<&str> = line.split('\x1E').collect();
            if parts.len() != 4 {
                bail!("Invalid git log format (expected 4 fields): {}", line);
            }
            let hash = parts[0].trim().to_string();
            let timestamp = chrono::DateTime::parse_from_rfc3339(parts[1].trim())
                .context("Failed to parse timestamp")?
                .with_timezone(&chrono::Utc);
            let message = parts[2].trim().to_string();
            let author = parts[3].trim().to_string();
            Ok(CommitData {
                hash,
                timestamp,
                message,
                author,
            })
        })
        .collect();

    Ok((commits?, temp_dir))
}

/// Convenience wrapper that discards the temporary clone directory.
async fn get_commits_with_full_metadata(
    repo_url: &str,
    branch: &str,
    limit: Option<usize>,
    since_commit: Option<&str>,
    creds: Option<&FlakeCredentialEnv>,
) -> Result<Vec<CommitData>> {
    let (commits, _dir) =
        get_commits_with_full_metadata_and_dir(repo_url, branch, limit, since_commit, creds)
            .await?;
    Ok(commits)
}

/// Read recent commit hashes from the remote branch in source-of-truth order.
///
/// This reflects `git log` ordering from the fetched branch and can be used
/// to filter stale database commits after force-push/rewrite events.
pub async fn get_recent_branch_commit_hashes_with_creds(
    repo_url: &str,
    branch: &str,
    limit: usize,
    creds: Option<&FlakeCredentialEnv>,
) -> Result<Vec<String>> {
    let commits =
        get_commits_with_full_metadata(repo_url, branch, Some(limit), None, creds).await?;
    Ok(commits.into_iter().map(|c| c.hash).collect())
}

/// Legacy function for backward compatibility - returns only hash and timestamp
async fn get_commits_with_timestamps(
    repo_url: &str,
    branch: &str,
    limit: Option<usize>,
    since_commit: Option<&str>,
) -> Result<Vec<(String, chrono::DateTime<chrono::Utc>)>> {
    let commits =
        get_commits_with_full_metadata(repo_url, branch, limit, since_commit, None).await?;
    Ok(commits.into_iter().map(|c| (c.hash, c.timestamp)).collect())
}

/// Fetch and insert all new commits since a given commit hash
pub async fn fetch_and_insert_commits_since(
    pool: &PgPool,
    repo_url: &str,
    branch: &str,
    since_commit: &Commit,
) -> Result<Vec<String>> {
    fetch_and_insert_commits_since_with_creds(pool, repo_url, branch, since_commit, None).await
}

/// Like [`fetch_and_insert_commits_since`] but accepts per-flake credentials.
pub async fn fetch_and_insert_commits_since_with_creds(
    pool: &PgPool,
    repo_url: &str,
    branch: &str,
    since_commit: &Commit,
    creds: Option<&FlakeCredentialEnv>,
) -> Result<Vec<String>> {
    let commits = match get_commits_with_full_metadata(
        repo_url,
        branch,
        Some(50),
        Some(&since_commit.git_commit_hash),
        creds,
    )
    .await
    {
        Ok(commits) => commits,
        Err(err) if is_invalid_revision_range_error(&err) => {
            warn!(
                repo_url = %repo_url,
                branch = %branch,
                since_hash = %since_commit.git_commit_hash,
                error = %err,
                "history_rewrite_detected via invalid revision range"
            );
            return Err(anyhow::anyhow!(
                "{}: remote history diverged for {} on {}. Last known commit {} is no longer in branch history. Accept rewrite via POST /api/v1/flakes/:id/accept-rewrite before syncing again. Root cause: {}",
                HISTORY_REWRITE_ERROR_MARKER,
                repo_url,
                branch,
                since_commit.git_commit_hash,
                err
            ));
        }
        Err(err) => return Err(err),
    };

    if commits.is_empty() {
        let remote_head_hash = remote_branch_head_hash(repo_url, branch, creds).await?;
        let diverged =
            is_remote_head_diverged(&since_commit.git_commit_hash, remote_head_hash.as_deref());

        info!(
            repo_url = %repo_url,
            branch = %branch,
            since_hash = %since_commit.git_commit_hash,
            remote_head_hash = ?remote_head_hash,
            diverged,
            "incremental sync produced zero commits; evaluated divergence"
        );

        if let Some(remote_head_hash) = remote_head_hash {
            if diverged {
                warn!(
                    repo_url = %repo_url,
                    branch = %branch,
                    since_hash = %since_commit.git_commit_hash,
                    remote_head_hash = %remote_head_hash,
                    "history_rewrite_detected via zero-update divergence"
                );
                return Err(anyhow::anyhow!(
                    "{}: remote history diverged for {} on {}. Last known commit {} no longer matches remote HEAD {}. Accept rewrite via POST /api/v1/flakes/:id/accept-rewrite before syncing again.",
                    HISTORY_REWRITE_ERROR_MARKER,
                    repo_url,
                    branch,
                    since_commit.git_commit_hash,
                    remote_head_hash,
                ));
            }
        }

        debug!(repo_url = %repo_url, branch = %branch, since_hash = %since_commit.git_commit_hash, "No new commits found and no divergence detected");
        return Ok(Vec::new());
    }

    let mut inserted = Vec::new();
    // Insert in reverse (oldest first) for chronological order
    for commit_data in commits.into_iter().rev() {
        match insert_commit_with_metadata(
            pool,
            &commit_data.hash,
            repo_url,
            commit_data.timestamp,
            Some(&commit_data.message),
            Some(&commit_data.author),
        )
        .await
        {
            Ok(n) if n > 0 => {
                debug!("✅ Inserted commit {} for {}", commit_data.hash, repo_url);
                inserted.push(commit_data.hash);
            }
            Ok(_) => {}
            Err(e) => warn!("Failed to insert commit {}: {}", commit_data.hash, e),
        }
    }

    info!(
        "✅ Inserted {} new commits for {}",
        inserted.len(),
        repo_url
    );
    Ok(inserted)
}

fn is_invalid_revision_range_error(err: &anyhow::Error) -> bool {
    err.to_string().contains("Invalid revision range")
}

pub fn is_history_rewrite_error(err: &anyhow::Error) -> bool {
    err.chain()
        .any(|cause| cause.to_string().contains(HISTORY_REWRITE_ERROR_MARKER))
}

async fn remote_branch_head_hash(
    repo_url: &str,
    branch: &str,
    creds: Option<&FlakeCredentialEnv>,
) -> Result<Option<String>> {
    let git_url = normalize_repo_url_for_git(repo_url, creds);
    let refspec = format!("refs/heads/{branch}");

    let mut cmd = tokio::process::Command::new("git");
    cmd.args(["ls-remote", &git_url, &refspec]);
    apply_optional_creds(&mut cmd, creds);

    let output = timeout(GIT_PROBE_TIMEOUT, cmd.output())
        .await
        .with_context(|| format!("Timed out probing remote HEAD for {repo_url} on {branch}"))?
        .with_context(|| format!("Failed to probe remote HEAD for {repo_url} on {branch}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "Git ls-remote failed for {repo_url} on {branch}: {}",
            stderr.trim()
        );
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let line = stdout.lines().find(|l| !l.trim().is_empty()).map(str::trim);

    let Some(line) = line else {
        return Ok(None);
    };

    let hash = line
        .split_whitespace()
        .next()
        .map(str::to_string)
        .filter(|value| !value.is_empty());

    Ok(hash)
}

fn is_remote_head_diverged(since_hash: &str, remote_head_hash: Option<&str>) -> bool {
    match remote_head_hash {
        Some(head) => head != since_hash,
        None => false,
    }
}

/// Resolve commit subject/author metadata for specific hashes.
///
/// Best effort: hashes that cannot be resolved are skipped.
pub async fn get_commit_metadata(
    repo_url: &str,
    commit_hashes: &[String],
) -> Result<HashMap<String, GitCommitMetadata>> {
    if commit_hashes.is_empty() {
        return Ok(HashMap::new());
    }

    let git_url = normalize_repo_url_for_git(repo_url, None);
    let temp_dir = tempfile::tempdir().context("Failed to create temporary directory")?;
    let clone_path = temp_dir.path();

    let clone_output = timeout(
        GIT_METADATA_TIMEOUT,
        tokio::process::Command::new("git")
            .args([
                "clone",
                "--depth",
                "200",
                "--filter=blob:none",
                &git_url,
                ".",
            ])
            .current_dir(clone_path)
            .output(),
    )
    .await
    .with_context(|| format!("Timed out cloning repo for metadata: {repo_url}"))?
    .with_context(|| format!("Failed to clone repo for metadata: {repo_url}"))?;

    if !clone_output.status.success() {
        let stderr = String::from_utf8_lossy(&clone_output.stderr);
        bail!("Git clone failed for {}: {}", repo_url, stderr.trim());
    }

    let prefetch = timeout(
        GIT_METADATA_TIMEOUT,
        tokio::process::Command::new("git")
            .args(["fetch", "--quiet", "--depth", "200", "origin"])
            .current_dir(clone_path)
            .output(),
    )
    .await;
    match prefetch {
        Ok(Ok(output)) if output.status.success() => {}
        Ok(Ok(output)) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            warn!(
                "Best-effort metadata prefetch failed for {}: {}",
                repo_url,
                stderr.trim()
            );
        }
        Ok(Err(err)) => {
            warn!(
                "Best-effort metadata prefetch failed for {}: {}",
                repo_url, err
            );
        }
        Err(_) => {
            warn!("Best-effort metadata prefetch timed out for {}", repo_url);
        }
    }

    let mut metadata = HashMap::new();
    for hash in commit_hashes {
        match load_commit_metadata(clone_path, hash).await {
            Ok(value) => {
                metadata.insert(hash.clone(), value);
            }
            Err(err) => {
                warn!("Failed to load git metadata for {}: {}", hash, err);
            }
        }
    }

    Ok(metadata)
}

/// Resolve `nixosConfigurations` names for specific commit hashes.
///
/// Best effort: commits that fail to evaluate are skipped.
/// Processes commits sequentially to avoid overwhelming nix eval.
pub async fn get_commit_nixos_configurations(
    repo_url: &str,
    commit_hashes: &[String],
) -> HashMap<String, Vec<String>> {
    let mut results = HashMap::new();

    // Limit to first 5 commits to avoid timeout cascade
    let limited_hashes = if commit_hashes.len() > 5 {
        warn!(
            "Limiting nixosConfigurations hydration to 5 commits (requested {})",
            commit_hashes.len()
        );
        &commit_hashes[..5]
    } else {
        commit_hashes
    };

    for hash in limited_hashes {
        match load_commit_nixos_configurations(repo_url, hash).await {
            Ok(configs) => {
                results.insert(hash.clone(), configs);
            }
            Err(err) => {
                warn!(
                    "Failed to resolve nixosConfigurations for {} @ {}: {}",
                    repo_url, hash, err
                );
            }
        }
    }

    results
}

/// Resolve changed file paths for specific commit hashes.
///
/// Best effort: commits that cannot be resolved are skipped.
pub async fn get_commit_changed_files(
    repo_url: &str,
    commit_hashes: &[String],
) -> Result<HashMap<String, Vec<String>>> {
    if commit_hashes.is_empty() {
        return Ok(HashMap::new());
    }

    let git_url = normalize_repo_url_for_git(repo_url, None);
    let temp_dir = tempfile::tempdir().context("Failed to create temporary directory")?;
    let clone_path = temp_dir.path();

    let clone_output = timeout(
        GIT_METADATA_TIMEOUT,
        tokio::process::Command::new("git")
            .args([
                "clone",
                "--depth",
                "200",
                "--filter=blob:none",
                &git_url,
                ".",
            ])
            .current_dir(clone_path)
            .output(),
    )
    .await
    .with_context(|| format!("Timed out cloning repo for changed files: {repo_url}"))?
    .with_context(|| format!("Failed to clone repo for changed files: {repo_url}"))?;

    if !clone_output.status.success() {
        let stderr = String::from_utf8_lossy(&clone_output.stderr);
        bail!("Git clone failed for {}: {}", repo_url, stderr.trim());
    }

    let mut changed = HashMap::new();
    for hash in commit_hashes {
        match load_commit_changed_files(clone_path, hash).await {
            Ok(files) => {
                changed.insert(hash.clone(), files);
            }
            Err(err) => {
                warn!("Failed to load changed files for {}: {}", hash, err);
            }
        }
    }

    Ok(changed)
}

async fn load_commit_nixos_configurations(
    repo_url: &str,
    commit_hash: &str,
) -> Result<Vec<String>> {
    let flake_ref = build_flake_reference(repo_url, commit_hash);
    let flake_target = format!("{flake_ref}#nixosConfigurations");

    let output = timeout(
        NIX_CONFIG_EVAL_TIMEOUT,
        tokio::process::Command::new("nix")
            .args([
                "eval",
                "--json",
                "--apply",
                "builtins.attrNames",
                flake_target.as_str(),
            ])
            .output(),
    )
    .await
    .with_context(|| format!("Timed out evaluating nixosConfigurations for {commit_hash}"))?
    .with_context(|| format!("Failed to evaluate nixosConfigurations for {commit_hash}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("nix eval failed for {}: {}", commit_hash, stderr.trim());
    }

    let mut names: Vec<String> = serde_json::from_slice(&output.stdout)
        .with_context(|| format!("Failed to parse nixosConfigurations JSON for {commit_hash}"))?;
    names.sort();
    names.dedup();
    Ok(names)
}

async fn load_commit_changed_files(
    clone_path: &std::path::Path,
    commit_hash: &str,
) -> Result<Vec<String>> {
    let output = timeout(
        GIT_METADATA_TIMEOUT,
        tokio::process::Command::new("git")
            .args(["show", "--pretty=format:", "--name-only", commit_hash])
            .current_dir(clone_path)
            .output(),
    )
    .await
    .with_context(|| format!("Timed out loading changed files for commit {commit_hash}"))?
    .with_context(|| format!("Failed to load changed files for commit {commit_hash}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "git show --name-only failed for {}: {}",
            commit_hash,
            stderr.trim()
        );
    }

    let mut files: Vec<String> = String::from_utf8(output.stdout)?
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
        .collect();
    files.sort();
    files.dedup();
    Ok(files)
}

async fn load_commit_metadata(
    clone_path: &std::path::Path,
    commit_hash: &str,
) -> Result<GitCommitMetadata> {
    let output = timeout(
        GIT_METADATA_TIMEOUT,
        tokio::process::Command::new("git")
            .args(["show", "-s", "--format=%H%x1f%s%x1f%an%x1f%ae", commit_hash])
            .current_dir(clone_path)
            .output(),
    )
    .await
    .with_context(|| format!("Timed out loading metadata for commit {commit_hash}"))?
    .with_context(|| format!("Failed to load metadata for commit {commit_hash}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("git show failed for {}: {}", commit_hash, stderr.trim());
    }

    let stdout = String::from_utf8(output.stdout)?;
    let line = stdout
        .lines()
        .find(|value| !value.trim().is_empty())
        .context("git show returned empty output")?;

    let mut parts = line.split('\u{1f}');
    let _hash = parts.next().context("Missing hash")?;
    let message = parts.next().context("Missing commit subject")?.trim();
    let author_name = parts.next().context("Missing author name")?.trim();
    let author_email = parts.next().unwrap_or("").trim();

    Ok(GitCommitMetadata {
        message: message.to_string(),
        author_name: author_name.to_string(),
        author_email: if author_email.is_empty() {
            None
        } else {
            Some(author_email.to_string())
        },
    })
}

/// Get the git diff for a specific commit.
/// Returns the full unified diff output from `git show`.
/// Tries multiple common branch names if the specified branch doesn't work.
/// Get the git diff for a specific commit.
/// Returns the full unified diff output from `git show`.
/// Tries multiple common branch names if the specified branch doesn't work.
pub async fn get_commit_diff(repo_url: &str, branch: &str, commit_hash: &str) -> Result<String> {
    get_commit_diff_with_creds(repo_url, branch, commit_hash, None).await
}

pub async fn get_commit_diff_with_creds(
    repo_url: &str,
    branch: &str,
    commit_hash: &str,
    creds: Option<&FlakeCredentialEnv>,
) -> Result<String> {
    let git_url = normalize_repo_url_for_git(repo_url, creds);
    let temp_dir = tempfile::tempdir().context("Failed to create temporary directory")?;
    let clone_path = temp_dir.path();

    // Try the specified branch first, then fall back to common branch names
    let branches_to_try = vec![
        branch.to_string(),
        "main".to_string(),
        "master".to_string(),
        "HEAD".to_string(),
    ];

    for branch_to_try in branches_to_try.iter() {
        let result =
            try_get_diff_for_branch(&git_url, clone_path, branch_to_try, commit_hash, creds).await;
        if let Ok(diff) = result {
            return Ok(diff);
        }
    }

    // If all branches fail, return an error
    let branch_list = branches_to_try.join(", ");
    bail!(
        "Could not find commit {} in any branch (tried: {})",
        commit_hash,
        branch_list
    )
}

async fn try_get_diff_for_branch(
    git_url: &str,
    clone_path: &std::path::Path,
    branch: &str,
    commit_hash: &str,
    creds: Option<&FlakeCredentialEnv>,
) -> Result<String> {
    // Clone with minimal depth since we only need one specific commit
    let mut clone_cmd = tokio::process::Command::new("git");
    clone_cmd
        .args([
            "clone",
            "--depth",
            "50", // Get enough depth to potentially find the commit
            "--branch",
            branch,
            "--single-branch",
            git_url,
            ".",
        ])
        .current_dir(clone_path);
    apply_optional_creds(&mut clone_cmd, creds);
    let clone_output = clone_cmd.output().await?;

    if !clone_output.status.success() {
        // Clone failed for this branch, try next one
        return Err(anyhow::anyhow!("Branch {} not found", branch));
    }

    // Try to get the diff for the commit
    let show_output = tokio::process::Command::new("git")
        .args(&[
            "show",
            "--format=", // Don't show commit message/metadata, just diff
            commit_hash,
        ])
        .current_dir(clone_path)
        .output()
        .await?;

    if !show_output.status.success() {
        // If the commit isn't in the shallow clone, try deepening first.
        // Some providers reject direct hash fetches even for reachable commits.
        let mut deepen_cmd = tokio::process::Command::new("git");
        deepen_cmd
            .args(["fetch", "--deepen", "1000", "origin", branch])
            .current_dir(clone_path);
        apply_optional_creds(&mut deepen_cmd, creds);
        let deepen_output = deepen_cmd.output().await?;

        if deepen_output.status.success() {
            let deepened_retry = tokio::process::Command::new("git")
                .args(&["show", "--format=", commit_hash])
                .current_dir(clone_path)
                .output()
                .await?;

            if deepened_retry.status.success() {
                return Ok(String::from_utf8_lossy(&deepened_retry.stdout).to_string());
            }
        }

        // Fallback: direct hash fetch for providers that allow it.
        let mut fetch_cmd = tokio::process::Command::new("git");
        fetch_cmd
            .args(["fetch", "origin", commit_hash])
            .current_dir(clone_path);
        apply_optional_creds(&mut fetch_cmd, creds);
        let fetch_output = fetch_cmd.output().await?;

        if fetch_output.status.success() {
            // Retry git show
            let retry_output = tokio::process::Command::new("git")
                .args(&["show", "--format=", commit_hash])
                .current_dir(clone_path)
                .output()
                .await?;

            if retry_output.status.success() {
                return Ok(String::from_utf8_lossy(&retry_output.stdout).to_string());
            }
        }

        // Last resort: fully unshallow and retry once.
        let mut unshallow_cmd = tokio::process::Command::new("git");
        unshallow_cmd
            .args(["fetch", "--unshallow", "origin", branch])
            .current_dir(clone_path);
        apply_optional_creds(&mut unshallow_cmd, creds);
        let unshallow_output = unshallow_cmd.output().await?;

        if unshallow_output.status.success() {
            let unshallow_retry = tokio::process::Command::new("git")
                .args(&["show", "--format=", commit_hash])
                .current_dir(clone_path)
                .output()
                .await?;

            if unshallow_retry.status.success() {
                return Ok(String::from_utf8_lossy(&unshallow_retry.stdout).to_string());
            }

            let stderr = String::from_utf8_lossy(&unshallow_retry.stderr);
            bail!("git show failed for {}: {}", commit_hash, stderr);
        }

        let show_stderr = String::from_utf8_lossy(&show_output.stderr);
        let fetch_stderr = String::from_utf8_lossy(&fetch_output.stderr);
        let deepen_stderr = String::from_utf8_lossy(&deepen_output.stderr);
        let unshallow_stderr = String::from_utf8_lossy(&unshallow_output.stderr);
        bail!(
            "Failed to resolve commit {} after shallow/deepen/fetch/unshallow retries. show='{}' deepen='{}' fetch='{}' unshallow='{}'",
            commit_hash,
            show_stderr.trim(),
            deepen_stderr.trim(),
            fetch_stderr.trim(),
            unshallow_stderr.trim()
        );
    }

    Ok(String::from_utf8_lossy(&show_output.stdout).to_string())
}

#[cfg(test)]
mod tests {
    use super::{
        is_history_rewrite_error, is_invalid_revision_range_error, is_remote_head_diverged,
        redact_sensitive_tokens, redact_url_credentials, sanitize_and_truncate_sync_error,
    };
    use anyhow::Context;

    #[test]
    fn detects_invalid_revision_range_error() {
        let err = anyhow::anyhow!("git log failed: fatal: Invalid revision range deadbeef..HEAD");
        assert!(is_invalid_revision_range_error(&err));
    }

    #[test]
    fn ignores_non_revision_range_errors() {
        let err = anyhow::anyhow!("git clone failed: authentication required");
        assert!(!is_invalid_revision_range_error(&err));
    }

    #[test]
    fn detects_history_rewrite_error_marker() {
        let err = anyhow::anyhow!("history_rewrite_detected: remote history diverged");
        assert!(is_history_rewrite_error(&err));
    }

    #[test]
    fn detects_history_rewrite_error_marker_in_error_chain() {
        let inner = anyhow::anyhow!("history_rewrite_detected: range diverged");
        let outer = inner.context("Failed to sync commits since last known hash");
        assert!(is_history_rewrite_error(&outer));
    }

    #[test]
    fn detects_remote_head_divergence_when_since_hash_differs() {
        assert!(is_remote_head_diverged("ec80a2f", Some("79e33a9")));
    }

    #[test]
    fn does_not_detect_divergence_when_hashes_match() {
        assert!(!is_remote_head_diverged("79e33a9", Some("79e33a9")));
    }

    #[test]
    fn does_not_detect_divergence_when_remote_head_missing() {
        assert!(!is_remote_head_diverged("79e33a9", None));
    }

    #[test]
    fn redacts_url_credentials_query_strings_and_fragments() {
        let redacted = redact_url_credentials(
            "https://user:p%40ss@git.example/repo.git?private_token=secret#access_token=secret2",
        );

        assert_eq!(redacted, "https://REDACTED@git.example/repo.git");
        assert!(!redacted.contains("p%40ss"));
        assert!(!redacted.contains("private_token"));
        assert!(!redacted.contains("access_token"));
    }

    #[test]
    fn strips_query_and_fragment_without_authority_credentials() {
        let redacted = redact_url_credentials(
            "https://git.example/repo.git?private_token=secret#access_token=secret2",
        );

        assert_eq!(redacted, "https://git.example/repo.git");
    }

    #[test]
    fn redacts_sensitive_patterns_case_insensitively() {
        let redacted = redact_sensitive_tokens(
            "authorization: bearer secret\ngit_askpass=/tmp/askpass netrc=/tmp/netrc NETRC_FILE=/tmp/netrc-file",
        );

        assert!(redacted.contains("Authorization: [REDACTED]"));
        assert!(redacted.contains("GIT_ASKPASS=[REDACTED]"));
        assert!(redacted.contains("NETRC=[REDACTED]"));
        assert!(redacted.contains("NETRC_FILE=[REDACTED]"));
        assert!(!redacted.contains("bearer secret"));
        assert!(!redacted.contains("/tmp/askpass"));
        assert!(!redacted.contains("/tmp/netrc"));
    }

    #[test]
    fn sanitizes_persisted_sync_errors_containing_repo_url_tokens() {
        let repo_url = "https://git.example/repo.git?private_token=secret#frag";
        let raw = "git failed for https://git.example/repo.git?private_token=secret#frag with authorization: bearer token";
        let sanitized = sanitize_and_truncate_sync_error(repo_url, raw, 4000);

        assert!(sanitized.contains("https://git.example/repo.git"));
        assert!(!sanitized.contains("private_token"));
        assert!(!sanitized.contains("#frag"));
        assert!(!sanitized.contains("bearer token"));
    }

    // ── Live-database supersession-race regression tests ────────────────────
    //
    // Run against a repository-provided isolated database:
    //   DATABASE_URL=postgres://crystal_forge:password@localhost:3042/crystal_forge \
    //     cargo test -p cf-server --lib flake::commits -- --ignored

    use super::{
        resolve_flake_attention_if_current, transition_flake_attention_to_error_if_current,
    };
    use crate::queries::attention;
    use crate::tasks::attention_reconciliation::reconcile_terminal_events;

    async fn test_pool() -> sqlx::PgPool {
        sqlx::PgPool::connect(
            &std::env::var("DATABASE_URL").expect("DATABASE_URL must be set for DB tests"),
        )
        .await
        .expect("failed to connect to test database")
    }

    async fn insert_throwaway_flake(pool: &sqlx::PgPool) -> i32 {
        let short = uuid::Uuid::new_v4().simple().to_string()[..12].to_string();
        sqlx::query_scalar::<_, i32>(
            "INSERT INTO flakes (name, repo_url, branch, sync_status) \
             VALUES ($1, $2, 'main', 'syncing') RETURNING id",
        )
        .bind(format!("att-flake-{short}"))
        .bind(format!("https://git.example/att-flake-{short}.git"))
        .fetch_one(pool)
        .await
        .expect("failed to insert throwaway test flake")
    }

    async fn open_flake_count(pool: &sqlx::PgPool, flake_id: i32) -> i64 {
        sqlx::query_scalar(
            "SELECT COUNT(*) FROM attention_occurrences \
             WHERE category = 'flakes' AND subject_id = $1 AND resolved_at IS NULL",
        )
        .bind(flake_id.to_string())
        .fetch_one(pool)
        .await
        .unwrap()
    }

    async fn insert_throwaway_user(pool: &sqlx::PgPool) -> uuid::Uuid {
        let user_id = uuid::Uuid::new_v4();
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

    async fn cleanup_user(pool: &sqlx::PgPool, user_id: uuid::Uuid) {
        let _ = sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(user_id)
            .execute(pool)
            .await;
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn resolve_flake_attention_if_current_skips_when_superseded_by_newer_error() {
        let pool = test_pool().await;
        let flake_id = insert_throwaway_flake(&pool).await;

        let stale_attempt_id = uuid::Uuid::new_v4();
        let newer_attempt_id = uuid::Uuid::new_v4();

        // Simulate: the newer attempt already recorded an error and opened
        // its attention occurrence.
        sqlx::query(
            "UPDATE flakes SET sync_status = 'error', sync_attempt_id = $2 WHERE id = $1",
        )
        .bind(flake_id)
        .bind(newer_attempt_id)
        .execute(&pool)
        .await
        .unwrap();

        transition_flake_attention_to_error_if_current(
            &pool,
            flake_id,
            newer_attempt_id,
            serde_json::json!({"flake_id": flake_id}),
        )
        .await;
        assert_eq!(
            open_flake_count(&pool, flake_id).await,
            1,
            "the newer attempt's sync_error occurrence must be open"
        );

        // The stale (delayed) success handler from an OLDER attempt must
        // not resolve the newer attempt's occurrence.
        resolve_flake_attention_if_current(&pool, flake_id, stale_attempt_id).await;
        assert_eq!(
            open_flake_count(&pool, flake_id).await,
            1,
            "a stale success handler must not resolve a newer attempt's occurrence"
        );

        let _ = sqlx::query("DELETE FROM attention_occurrences WHERE category = 'flakes' AND subject_id = $1")
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
    async fn resolve_flake_attention_if_current_resolves_when_still_current() {
        let pool = test_pool().await;
        let flake_id = insert_throwaway_flake(&pool).await;
        let attempt_id = uuid::Uuid::new_v4();

        // Open a sync_error occurrence for this exact attempt.
        sqlx::query(
            "UPDATE flakes SET sync_status = 'error', sync_attempt_id = $2 WHERE id = $1",
        )
        .bind(flake_id)
        .bind(attempt_id)
        .execute(&pool)
        .await
        .unwrap();
        transition_flake_attention_to_error_if_current(
            &pool,
            flake_id,
            attempt_id,
            serde_json::json!({"flake_id": flake_id}),
        )
        .await;
        assert_eq!(open_flake_count(&pool, flake_id).await, 1);

        // The same attempt later succeeds and resolves its own occurrence.
        sqlx::query("UPDATE flakes SET sync_status = 'synced' WHERE id = $1")
            .bind(flake_id)
            .execute(&pool)
            .await
            .unwrap();
        resolve_flake_attention_if_current(&pool, flake_id, attempt_id).await;
        assert_eq!(
            open_flake_count(&pool, flake_id).await,
            0,
            "the current attempt's own resolve must succeed"
        );

        let _ = sqlx::query("DELETE FROM attention_occurrences WHERE category = 'flakes' AND subject_id = $1")
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
    async fn transition_flake_attention_to_error_if_current_skips_when_superseded() {
        let pool = test_pool().await;
        let flake_id = insert_throwaway_flake(&pool).await;

        let stale_attempt_id = uuid::Uuid::new_v4();
        let newer_attempt_id = uuid::Uuid::new_v4();

        // Newer attempt has since succeeded.
        sqlx::query(
            "UPDATE flakes SET sync_status = 'synced', sync_attempt_id = $2 WHERE id = $1",
        )
        .bind(flake_id)
        .bind(newer_attempt_id)
        .execute(&pool)
        .await
        .unwrap();

        // A delayed error handler from a stale, superseded attempt must not
        // open a sync_error occurrence.
        transition_flake_attention_to_error_if_current(
            &pool,
            flake_id,
            stale_attempt_id,
            serde_json::json!({"flake_id": flake_id}),
        )
        .await;
        assert_eq!(
            open_flake_count(&pool, flake_id).await,
            0,
            "a stale error handler must not open an occurrence after a newer success"
        );

        let _ = sqlx::query("DELETE FROM attention_occurrences WHERE category = 'flakes' AND subject_id = $1")
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
    async fn transition_flake_attention_to_error_if_current_does_not_inherit_dismissal_across_intervening_success() {
        // Regression test for round 9: attempt A fails and opens (then the
        // user dismisses) occurrence O. Attempt B succeeds, but its
        // resolve_flake_attention_if_current call is lost (simulated here
        // by never calling it), so O is never resolved. Attempt C later
        // fails. Without the last_synced_at staleness check, C would find
        // O still open with a matching reason and simply observe/reuse it
        // -- silently inheriting A's dismissal for what is, from the
        // user's perspective, a brand new failure they have never seen.
        let pool = test_pool().await;
        let flake_id = insert_throwaway_flake(&pool).await;
        let user_id = insert_throwaway_user(&pool).await;

        let attempt_a = uuid::Uuid::new_v4();
        let attempt_b = uuid::Uuid::new_v4();
        let attempt_c = uuid::Uuid::new_v4();

        // Attempt A fails, opens occurrence O.
        sqlx::query("UPDATE flakes SET sync_status = 'error', sync_attempt_id = $2 WHERE id = $1")
            .bind(flake_id)
            .bind(attempt_a)
            .execute(&pool)
            .await
            .unwrap();
        transition_flake_attention_to_error_if_current(
            &pool,
            flake_id,
            attempt_a,
            serde_json::json!({"flake_id": flake_id}),
        )
        .await;
        let o: uuid::Uuid = sqlx::query_scalar(
            "SELECT id FROM attention_occurrences WHERE category = 'flakes' AND subject_id = $1 AND resolved_at IS NULL",
        )
        .bind(flake_id.to_string())
        .fetch_one(&pool)
        .await
        .unwrap();

        // User dismisses O.
        sqlx::query(
            "INSERT INTO user_attention_dismissals (user_id, occurrence_id, dismissed_at) VALUES ($1, $2, NOW())",
        )
        .bind(user_id)
        .bind(o)
        .execute(&pool)
        .await
        .unwrap();

        // Attempt B succeeds -- sets last_synced_at, but its attention
        // resolution is simulated as lost (never called).
        sqlx::query(
            "UPDATE flakes SET sync_status = 'synced', sync_attempt_id = $2, last_synced_at = now() WHERE id = $1",
        )
        .bind(flake_id)
        .bind(attempt_b)
        .execute(&pool)
        .await
        .unwrap();

        // Attempt C fails.
        sqlx::query("UPDATE flakes SET sync_status = 'error', sync_attempt_id = $2 WHERE id = $1")
            .bind(flake_id)
            .bind(attempt_c)
            .execute(&pool)
            .await
            .unwrap();
        transition_flake_attention_to_error_if_current(
            &pool,
            flake_id,
            attempt_c,
            serde_json::json!({"flake_id": flake_id}),
        )
        .await;

        // A NEW occurrence must exist, distinct from O, and NOT dismissed.
        let current: (uuid::Uuid, bool) = sqlx::query_as(
            "SELECT ao.id, EXISTS (SELECT 1 FROM user_attention_dismissals uad WHERE uad.user_id = $2 AND uad.occurrence_id = ao.id) \
             FROM attention_occurrences ao \
             WHERE ao.category = 'flakes' AND ao.subject_id = $1 AND ao.resolved_at IS NULL",
        )
        .bind(flake_id.to_string())
        .bind(user_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_ne!(
            current.0, o,
            "attempt C must open a NEW occurrence, not reuse O from attempt A"
        );
        assert!(
            !current.1,
            "the new occurrence for attempt C must NOT inherit attempt A's dismissal"
        );

        // O itself must now be resolved (superseded).
        let o_resolved: Option<chrono::DateTime<chrono::Utc>> =
            sqlx::query_scalar("SELECT resolved_at FROM attention_occurrences WHERE id = $1")
                .bind(o)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(o_resolved.is_some(), "O must be resolved as superseded");

        let _ = sqlx::query("DELETE FROM attention_occurrences WHERE category = 'flakes' AND subject_id = $1")
            .bind(flake_id.to_string())
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
    async fn transition_flake_attention_to_error_if_current_reuses_occurrence_without_intervening_success() {
        // Sanity counterpart: when NO success has occurred since the
        // existing occurrence was last observed, it IS safe to reuse
        // (observe) it -- this is the normal, common case of a flake
        // failing repeatedly without ever recovering in between.
        let pool = test_pool().await;
        let flake_id = insert_throwaway_flake(&pool).await;

        let attempt_a = uuid::Uuid::new_v4();
        let attempt_b = uuid::Uuid::new_v4();

        sqlx::query("UPDATE flakes SET sync_status = 'error', sync_attempt_id = $2 WHERE id = $1")
            .bind(flake_id)
            .bind(attempt_a)
            .execute(&pool)
            .await
            .unwrap();
        transition_flake_attention_to_error_if_current(
            &pool,
            flake_id,
            attempt_a,
            serde_json::json!({"flake_id": flake_id}),
        )
        .await;
        let o: uuid::Uuid = sqlx::query_scalar(
            "SELECT id FROM attention_occurrences WHERE category = 'flakes' AND subject_id = $1 AND resolved_at IS NULL",
        )
        .bind(flake_id.to_string())
        .fetch_one(&pool)
        .await
        .unwrap();

        // Attempt B fails too, with no intervening success (last_synced_at
        // remains NULL / unchanged).
        sqlx::query("UPDATE flakes SET sync_status = 'error', sync_attempt_id = $2 WHERE id = $1")
            .bind(flake_id)
            .bind(attempt_b)
            .execute(&pool)
            .await
            .unwrap();
        transition_flake_attention_to_error_if_current(
            &pool,
            flake_id,
            attempt_b,
            serde_json::json!({"flake_id": flake_id}),
        )
        .await;

        let still_open: uuid::Uuid = sqlx::query_scalar(
            "SELECT id FROM attention_occurrences WHERE category = 'flakes' AND subject_id = $1 AND resolved_at IS NULL",
        )
        .bind(flake_id.to_string())
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            still_open, o,
            "with no intervening success, the SAME occurrence must be reused, not a new one opened"
        );

        let _ = sqlx::query("DELETE FROM attention_occurrences WHERE category = 'flakes' AND subject_id = $1")
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
    async fn transition_flake_attention_to_error_if_current_skips_deleted_flake() {
        // Regression test for round 11: a delayed error transition (e.g.
        // `record_sync_error` scheduled late relative to a concurrent
        // soft/hard delete of the same flake) must not resurrect an
        // attention occurrence for a flake that has since been deleted,
        // even if `sync_attempt_id` and `sync_status` still match what the
        // caller expects -- the recheck query now requires
        // `deleted_at IS NULL`, so a deleted row is treated the same as a
        // missing one (no-op).
        let pool = test_pool().await;
        let flake_id = insert_throwaway_flake(&pool).await;
        let attempt_id = uuid::Uuid::new_v4();

        // Simulate: flake recorded an error, then was deleted directly
        // (bypassing soft_delete_flake, to reproduce exactly the stale
        // state a delayed caller could observe) WITHOUT its attention
        // state changing.
        sqlx::query(
            "UPDATE flakes SET sync_status = 'error', sync_attempt_id = $2, deleted_at = NOW() WHERE id = $1",
        )
        .bind(flake_id)
        .bind(attempt_id)
        .execute(&pool)
        .await
        .unwrap();

        transition_flake_attention_to_error_if_current(
            &pool,
            flake_id,
            attempt_id,
            serde_json::json!({"flake_id": flake_id}),
        )
        .await;

        assert_eq!(
            open_flake_count(&pool, flake_id).await,
            0,
            "a delayed error transition must not create an occurrence for a deleted flake"
        );

        let _ = sqlx::query("DELETE FROM attention_occurrences WHERE category = 'flakes' AND subject_id = $1")
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
    async fn resolve_flake_attention_if_current_skips_deleted_flake() {
        // Regression test for round 11: the deleted_at guard is applied
        // consistently to both flake attention recheck helpers. Verifies
        // the guard is actually wired up in resolve_flake_attention_if_current
        // (the delete paths themselves are already responsible for
        // resolving any open occurrence directly, under their own lock —
        // this only confirms the recheck's `deleted_at IS NULL` predicate
        // takes effect for a delayed caller racing a delete).
        let pool = test_pool().await;
        let flake_id = insert_throwaway_flake(&pool).await;
        let attempt_id = uuid::Uuid::new_v4();

        sqlx::query("UPDATE flakes SET sync_status = 'error', sync_attempt_id = $2 WHERE id = $1")
            .bind(flake_id)
            .bind(attempt_id)
            .execute(&pool)
            .await
            .unwrap();
        transition_flake_attention_to_error_if_current(
            &pool,
            flake_id,
            attempt_id,
            serde_json::json!({"flake_id": flake_id}),
        )
        .await;
        assert_eq!(open_flake_count(&pool, flake_id).await, 1);

        // Delete the flake directly (bypassing soft_delete_flake, which
        // would itself resolve the occurrence) so the occurrence remains
        // open, simulating a delayed caller racing a delete performed by
        // some other path.
        sqlx::query("UPDATE flakes SET sync_status = 'synced', deleted_at = NOW() WHERE id = $1")
            .bind(flake_id)
            .execute(&pool)
            .await
            .unwrap();

        resolve_flake_attention_if_current(&pool, flake_id, attempt_id).await;

        assert_eq!(
            open_flake_count(&pool, flake_id).await,
            1,
            "resolve_flake_attention_if_current must not act on a deleted flake row \
             (the deleted_at guard makes still_current false, so this is a no-op)"
        );

        let _ = sqlx::query("DELETE FROM attention_occurrences WHERE category = 'flakes' AND subject_id = $1")
            .bind(flake_id.to_string())
            .execute(&pool)
            .await;
        let _ = sqlx::query("DELETE FROM flakes WHERE id = $1")
            .bind(flake_id)
            .execute(&pool)
            .await;
    }

    async fn insert_throwaway_commit_for_flake(pool: &sqlx::PgPool, flake_id: i32) -> i32 {
        let short = uuid::Uuid::new_v4().simple().to_string()[..12].to_string();
        let hash = format!("{short}{short}{short}{short}");
        sqlx::query_scalar::<_, i32>(
            "INSERT INTO commits (flake_id, git_commit_hash, message, author, commit_timestamp) \
             VALUES ($1, $2, $3, $4, NOW()) RETURNING id",
        )
        .bind(flake_id)
        .bind(&hash)
        .bind(format!("test commit {short}"))
        .bind("test author")
        .fetch_one(pool)
        .await
        .expect("failed to insert throwaway test commit")
    }

    async fn insert_throwaway_build_for_commit(
        pool: &sqlx::PgPool,
        commit_id: i32,
    ) -> (uuid::Uuid, chrono::DateTime<chrono::Utc>) {
        let short = uuid::Uuid::new_v4().simple().to_string()[..12].to_string();
        let derivation_id: i32 = sqlx::query_scalar(
            "INSERT INTO derivations (commit_id, derivation_type, derivation_name, status_id, attempt_count) \
             VALUES ($1, 'package', $2, 11, 0) RETURNING id",
        )
        .bind(commit_id)
        .bind(format!("test-drv-{short}"))
        .fetch_one(pool)
        .await
        .expect("failed to insert throwaway test derivation");

        let job_id = uuid::Uuid::new_v4();
        let completed_at = chrono::Utc::now();
        sqlx::query(
            "INSERT INTO build_jobs (id, derivation_id, status, completed_at) \
             VALUES ($1, $2, 'failed', $3)",
        )
        .bind(job_id)
        .bind(derivation_id)
        .bind(completed_at)
        .execute(pool)
        .await
        .expect("failed to insert throwaway test build job");

        (job_id, completed_at)
    }

    async fn insert_snapshot_entry(
        pool: &sqlx::PgPool,
        flake_id: i32,
        commit_id: i32,
        position: i32,
    ) {
        sqlx::query(
            "INSERT INTO flake_branch_commit_snapshot (flake_id, commit_id, position, observed_at) \
             VALUES ($1, $2, $3, NOW()) \
             ON CONFLICT (flake_id, commit_id) DO UPDATE SET position = EXCLUDED.position",
        )
        .bind(flake_id)
        .bind(commit_id)
        .bind(position)
        .execute(pool)
        .await
        .expect("failed to insert snapshot entry");
    }

    async fn remove_snapshot_entry(pool: &sqlx::PgPool, flake_id: i32, commit_id: i32) {
        sqlx::query(
            "DELETE FROM flake_branch_commit_snapshot WHERE flake_id = $1 AND commit_id = $2",
        )
        .bind(flake_id)
        .bind(commit_id)
        .execute(pool)
        .await
        .expect("failed to remove snapshot entry");
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn reconcile_terminal_events_resolves_eval_for_commit_removed_from_snapshot() {
        let pool = test_pool().await;
        let flake_id = insert_throwaway_flake(&pool).await;
        let commit_id = insert_throwaway_commit_for_flake(&pool, flake_id).await;

        let completed_at = chrono::Utc::now();
        let key = attention::eval_occurrence_key(commit_id, completed_at);
        sqlx::query(
            "INSERT INTO attention_occurrences (category, subject_type, subject_id, \
             source_occurrence_key, opened_at, last_observed_at, metadata) \
             VALUES ('evals', 'commit_eval', $1, $2, $3, $3, $4)",
        )
        .bind(commit_id.to_string())
        .bind(&key)
        .bind(completed_at)
        .bind(serde_json::json!({"commit_id": commit_id}))
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query(
            "UPDATE commits SET evaluation_status = 'failed', evaluation_completed_at = $1 \
             WHERE id = $2",
        )
        .bind(completed_at)
        .bind(commit_id)
        .execute(&pool)
        .await
        .unwrap();

        // Commit is in the active snapshot: terminal reconciliation should
        // keep the occurrence.
        sqlx::query(
            "INSERT INTO flake_branch_commit_snapshot (flake_id, commit_id, position, observed_at) \
             VALUES ($1, $2, 0, NOW())",
        )
        .bind(flake_id)
        .bind(commit_id)
        .execute(&pool)
        .await
        .unwrap();

        reconcile_terminal_events(&pool).await;

        let open_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM attention_occurrences \
             WHERE category = 'evals' AND subject_id = $1 AND resolved_at IS NULL",
        )
        .bind(commit_id.to_string())
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            open_count, 1,
            "eval occurrence must remain while commit is in the active snapshot"
        );

        // Simulate a history rewrite removing the commit from the snapshot.
        sqlx::query(
            "DELETE FROM flake_branch_commit_snapshot \
             WHERE flake_id = $1 AND commit_id = $2",
        )
        .bind(flake_id)
        .bind(commit_id)
        .execute(&pool)
        .await
        .unwrap();

        // Terminal reconciliation should now resolve the archived occurrence.
        reconcile_terminal_events(&pool).await;

        let open_count_after: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM attention_occurrences \
             WHERE category = 'evals' AND subject_id = $1 AND resolved_at IS NULL",
        )
        .bind(commit_id.to_string())
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            open_count_after, 0,
            "eval occurrence must be resolved once commit leaves the active snapshot"
        );

        let _ = sqlx::query(
            "DELETE FROM attention_occurrences WHERE category = 'evals' AND subject_id = $1",
        )
        .bind(commit_id.to_string())
        .execute(&pool)
        .await;
        let _ = sqlx::query("DELETE FROM flake_branch_commit_snapshot WHERE flake_id = $1")
            .bind(flake_id)
            .execute(&pool)
            .await;
        let _ = sqlx::query("DELETE FROM commits WHERE flake_id = $1")
            .bind(flake_id)
            .execute(&pool)
            .await;
        let _ = sqlx::query("DELETE FROM flakes WHERE id = $1")
            .bind(flake_id)
            .execute(&pool)
            .await;
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn reconcile_terminal_events_does_not_reopen_eval_for_removed_commit() {
        // Regression test for round 17 issue 3: terminal reconciliation must
        // not open a new eval occurrence for a commit that has left the
        // active branch snapshot, even if the previous occurrence was lost.
        let pool = test_pool().await;
        let flake_id = insert_throwaway_flake(&pool).await;
        let commit_id = insert_throwaway_commit_for_flake(&pool, flake_id).await;

        let completed_at = chrono::Utc::now();
        sqlx::query(
            "UPDATE commits SET evaluation_status = 'failed', evaluation_completed_at = $1 \
             WHERE id = $2",
        )
        .bind(completed_at)
        .bind(commit_id)
        .execute(&pool)
        .await
        .unwrap();

        // First pass: commit is in the snapshot, so an occurrence is opened.
        insert_snapshot_entry(&pool, flake_id, commit_id, 0).await;
        reconcile_terminal_events(&pool).await;

        let open_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM attention_occurrences \
             WHERE category = 'evals' AND subject_id = $1 AND resolved_at IS NULL",
        )
        .bind(commit_id.to_string())
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(open_count, 1, "eval occurrence must be opened while commit is in snapshot");

        // Simulate history rewrite: remove the commit from the snapshot and
        // delete the occurrence (e.g. the producer/rewrite resolved it).
        remove_snapshot_entry(&pool, flake_id, commit_id).await;
        let _ = sqlx::query(
            "DELETE FROM attention_occurrences WHERE category = 'evals' AND subject_id = $1",
        )
        .bind(commit_id.to_string())
        .execute(&pool)
        .await;

        // Second pass: the commit is no longer in the snapshot, so the atomic
        // recheck in open_eval_attention_if_current must not reopen attention.
        reconcile_terminal_events(&pool).await;

        let open_count_after: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM attention_occurrences \
             WHERE category = 'evals' AND subject_id = $1 AND resolved_at IS NULL",
        )
        .bind(commit_id.to_string())
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            open_count_after, 0,
            "eval occurrence must NOT be reopened once commit leaves the active snapshot"
        );

        let _ = sqlx::query("DELETE FROM attention_occurrences WHERE category = 'evals' AND subject_id = $1")
            .bind(commit_id.to_string())
            .execute(&pool)
            .await;
        let _ = sqlx::query("DELETE FROM flake_branch_commit_snapshot WHERE flake_id = $1")
            .bind(flake_id)
            .execute(&pool)
            .await;
        let _ = sqlx::query("DELETE FROM commits WHERE flake_id = $1")
            .bind(flake_id)
            .execute(&pool)
            .await;
        let _ = sqlx::query("DELETE FROM flakes WHERE id = $1")
            .bind(flake_id)
            .execute(&pool)
            .await;
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn reconcile_terminal_events_does_not_reopen_build_for_removed_commit() {
        // Regression test for round 17 issue 3: terminal reconciliation must
        // not open a new build occurrence for a build whose commit has left
        // the active branch snapshot.
        let pool = test_pool().await;
        let flake_id = insert_throwaway_flake(&pool).await;
        let commit_id = insert_throwaway_commit_for_flake(&pool, flake_id).await;
        let (job_id, completed_at) = insert_throwaway_build_for_commit(&pool, commit_id).await;

        // First pass: commit is in the snapshot, so an occurrence is opened.
        insert_snapshot_entry(&pool, flake_id, commit_id, 0).await;
        reconcile_terminal_events(&pool).await;

        let open_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM attention_occurrences \
             WHERE category = 'builds' AND subject_id = $1 AND resolved_at IS NULL",
        )
        .bind(job_id.to_string())
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(open_count, 1, "build occurrence must be opened while commit is in snapshot");

        // Simulate history rewrite removing the commit.
        remove_snapshot_entry(&pool, flake_id, commit_id).await;
        let _ = sqlx::query(
            "DELETE FROM attention_occurrences WHERE category = 'builds' AND subject_id = $1",
        )
        .bind(job_id.to_string())
        .execute(&pool)
        .await;

        // Second pass: the atomic recheck must not reopen the build occurrence.
        reconcile_terminal_events(&pool).await;

        let open_count_after: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM attention_occurrences \
             WHERE category = 'builds' AND subject_id = $1 AND resolved_at IS NULL",
        )
        .bind(job_id.to_string())
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            open_count_after, 0,
            "build occurrence must NOT be reopened once commit leaves the active snapshot"
        );

        let _ = sqlx::query("DELETE FROM attention_occurrences WHERE category = 'builds' AND subject_id = $1")
            .bind(job_id.to_string())
            .execute(&pool)
            .await;
        let _ = sqlx::query("DELETE FROM build_jobs WHERE id = $1")
            .bind(job_id)
            .execute(&pool)
            .await;
        let _ = sqlx::query("DELETE FROM derivations WHERE commit_id = $1")
            .bind(commit_id)
            .execute(&pool)
            .await;
        let _ = sqlx::query("DELETE FROM flake_branch_commit_snapshot WHERE flake_id = $1")
            .bind(flake_id)
            .execute(&pool)
            .await;
        let _ = sqlx::query("DELETE FROM commits WHERE flake_id = $1")
            .bind(flake_id)
            .execute(&pool)
            .await;
        let _ = sqlx::query("DELETE FROM flakes WHERE id = $1")
            .bind(flake_id)
            .execute(&pool)
            .await;
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn reconcile_terminal_events_serializes_with_flake_sync_lock() {
        // Race test for round 17 issue 3: while reconcile_terminal_events is
        // opening attention for a failed commit, a concurrent history rewrite
        // removes the commit from the snapshot. The per-flake advisory sync
        // lock serializes the two operations so the recheck inside the open
        // transaction observes the post-rewrite state and does not reopen
        // attention.
        let pool = test_pool().await;
        let flake_id = insert_throwaway_flake(&pool).await;
        let commit_id = insert_throwaway_commit_for_flake(&pool, flake_id).await;

        let completed_at = chrono::Utc::now();
        sqlx::query(
            "UPDATE commits SET evaluation_status = 'failed', evaluation_completed_at = $1 \
             WHERE id = $2",
        )
        .bind(completed_at)
        .bind(commit_id)
        .execute(&pool)
        .await
        .unwrap();

        insert_snapshot_entry(&pool, flake_id, commit_id, 0).await;

        let lock_key = crate::queries::commits::SYNC_LOCK_BASE + i64::from(flake_id);

        // Task A: acquire the flake sync lock, wait until the reconciler is
        // known to be blocked, then remove the commit from the snapshot and
        // release the lock.
        let pool_a = pool.clone();
        let rewrite = tokio::spawn(async move {
            let mut tx = pool_a.begin().await.expect("begin rewrite tx");
            sqlx::query("SELECT pg_advisory_xact_lock($1)")
                .bind(lock_key)
                .execute(&mut *tx)
                .await
                .expect("acquire flake sync lock in rewrite");

            // Give reconcile_terminal_events enough time to start and reach
            // the lock acquisition for the candidate.
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;

            remove_snapshot_entry(&pool_a, flake_id, commit_id).await;

            tx.commit().await.expect("commit rewrite tx");
        });

        // Task B: start reconciliation. Its SELECT for candidates does not
        // need the lock, so it will find the commit; the open transaction will
        // block on the lock until the rewrite commits.
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let reconcile = tokio::spawn({
            let pool_b = pool.clone();
            async move {
                reconcile_terminal_events(&pool_b).await;
            }
        });

        // Both must complete without error.
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            rewrite.await.unwrap();
            reconcile.await.unwrap();
        })
        .await
        .expect("rewrite and reconcile must complete within timeout");

        // The post-rewrite recheck must have prevented reopening attention.
        let open_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM attention_occurrences \
             WHERE category = 'evals' AND subject_id = $1 AND resolved_at IS NULL",
        )
        .bind(commit_id.to_string())
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            open_count, 0,
            "eval occurrence must not be reopened when rewrite holds the sync lock and removes the commit"
        );

        let _ = sqlx::query("DELETE FROM attention_occurrences WHERE category = 'evals' AND subject_id = $1")
            .bind(commit_id.to_string())
            .execute(&pool)
            .await;
        let _ = sqlx::query("DELETE FROM flake_branch_commit_snapshot WHERE flake_id = $1")
            .bind(flake_id)
            .execute(&pool)
            .await;
        let _ = sqlx::query("DELETE FROM commits WHERE flake_id = $1")
            .bind(flake_id)
            .execute(&pool)
            .await;
        let _ = sqlx::query("DELETE FROM flakes WHERE id = $1")
            .bind(flake_id)
            .execute(&pool)
            .await;
    }
}
