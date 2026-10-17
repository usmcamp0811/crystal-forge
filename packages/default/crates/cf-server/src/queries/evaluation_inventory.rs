//! Captures immutable post-discovery inventory under exact attempt ownership.
//!
//! Missing inventory means unavailable discovery, whereas empty arrays record a
//! successful empty discovery. Selection records scope, never execution success.

use anyhow::{Result, bail};
use sqlx::PgPool;
use std::time::Duration;

use crate::models::evaluate_with_policies::{EvaluationCancelled, EvaluationSuperseded};

/// Bounds foreground required-inventory persistence, including pool acquisition.
///
/// Matches the configured 30-second database pool acquisition allowance, but
/// applies once to the whole write rather than separately to each statement.
pub(crate) const REQUIRED_INVENTORY_WRITE_BUDGET: Duration = Duration::from_secs(30);

/// Separates the database statement ceiling from the foreground write deadline.
///
/// PostgreSQL gets one additional second to finish an in-flight statement after
/// the caller's whole-write budget. A dropped SQLx transaction queues rollback
/// behind that statement; the statement must end even if its blocker stays held.
pub(crate) const REQUIRED_INVENTORY_CLEANUP_GRACE: Duration = Duration::from_secs(1);

/// Identifies the source of inventory names without claiming outcome evidence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InventoryExecutionMode {
    /// Names returned by discovery of the verified immutable native source.
    Native,
    /// Names chosen by the synthetic mock pipeline without native discovery.
    Mock,
}

impl InventoryExecutionMode {
    fn as_str(self) -> &'static str {
        match self {
            Self::Native => "native",
            Self::Mock => "mock",
        }
    }
}

/// Reports an attempt to replace an existing inventory with different evidence.
#[derive(Debug)]
pub struct InventoryConflict;

impl std::fmt::Display for InventoryConflict {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("evaluation attempt inventory differs from its immutable first write")
    }
}

impl std::error::Error for InventoryConflict {}

fn normalized(names: &[String]) -> Vec<String> {
    let mut names = names.to_vec();
    names.sort();
    names.dedup();
    names
}

/// Persists the first inventory for the current started evaluation attempt.
///
/// Names retain their exact spelling and are sorted and deduplicated. Selected
/// names MUST be a subset of discovered names. Empty inventories are valid.
/// Identical repeated writes are idempotent only while the attempt remains
/// current and active. A queued retry makes the older attempt unavailable for
/// writes even before the started counter advances. No cache is consulted.
///
/// Transaction-local lock and statement timeouts limit each database statement
/// to at most 31 seconds (the 30-second foreground budget plus one cleanup
/// second). Each setting preserves a stricter existing nonzero database, role,
/// or session timeout. Zero means disabled and receives the 31-second ceiling.
/// These limits let PostgreSQL abort a blocked statement and process SQLx's
/// queued rollback after the caller drops the write, without releasing the
/// external blocker. They do not bound pool acquisition or total transaction
/// time; required-write callers must enforce the separate whole-write deadline.
/// Rollback acknowledgement and connection reuse still require a responsive
/// database and executor. An unknown COMMIT result does not prove row absence.
///
/// # Errors
/// Returns `EvaluationSuperseded` for missing, nonactive, or older ownership,
/// [`EvaluationCancelled`] for cancellation of the owned active attempt, and
/// [`InventoryConflict`] for differing first-write evidence. Returns validation
/// errors for selections outside discovery, and database errors on persistence
/// failure, including database lock/statement timeout. An error never replaces
/// existing inventory.
///
/// # Examples
/// ```no_run
/// use crystal_forge::queries::evaluation_inventory::{
///     InventoryExecutionMode, persist_inventory,
/// };
///
/// # async fn capture(pool: &sqlx::PgPool) -> anyhow::Result<()> {
/// let discovered = vec!["web-server".to_owned()];
/// // Commit 42 must own active attempt 1 before discovery is persisted.
/// persist_inventory(pool, 42, 1, &discovered, &discovered,
///     InventoryExecutionMode::Native).await?;
/// # Ok(())
/// # }
/// ```
pub async fn persist_inventory(
    pool: &PgPool,
    commit_id: i32,
    expected_attempt: i32,
    discovered: &[String],
    selected: &[String],
    mode: InventoryExecutionMode,
) -> Result<()> {
    let discovered = normalized(discovered);
    let selected = normalized(selected);
    if selected
        .iter()
        .any(|name| discovered.binary_search(name).is_err())
    {
        bail!("selected configuration names must be a subset of discovered names");
    }
    let mut tx = pool.begin().await?;
    // CONCURRENCY: Dropping a SQLx query does not cancel PostgreSQL's lock wait.
    // Bound the statement before taking any writer locks so a queued rollback
    // can run while an external snapshot writer still holds its advisory lock.
    // pg_settings normalizes both settings to milliseconds. Never expand a
    // stricter nonzero policy. VALUES plus LEFT JOIN applies the ceiling even
    // if a settings lookup is missing. Both settings reset on commit/rollback.
    let statement_timeout_ms = i64::try_from(
        (REQUIRED_INVENTORY_WRITE_BUDGET + REQUIRED_INVENTORY_CLEANUP_GRACE).as_millis(),
    )?;
    sqlx::query(
        "SELECT set_config(requested.name,
             LEAST(COALESCE(NULLIF(settings.setting::bigint, 0), $1::bigint),
                 $1::bigint)::text || 'ms', true)
         FROM (VALUES ('lock_timeout'), ('statement_timeout')) AS requested(name)
         LEFT JOIN pg_catalog.pg_settings AS settings ON settings.name = requested.name",
    )
    .bind(statement_timeout_ms)
    .execute(&mut *tx)
    .await?;
    // CONCURRENCY: Match retry/finalizer lock ordering. Commit ownership stays
    // locked through insertion and comparison, preventing a late older writer.
    super::evaluation_snapshots::lock_snapshot_writer_tx(&mut tx).await?;
    let state: Option<(Option<String>, Option<i32>, Option<bool>)> = sqlx::query_as(
        "SELECT evaluation_status, evaluation_attempt_count, cancellation_requested
         FROM commits WHERE id = $1 FOR UPDATE",
    )
    .bind(commit_id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((status, attempt, cancelled)) = state else {
        return Err(EvaluationSuperseded.into());
    };
    if attempt != Some(expected_attempt)
        || !matches!(status.as_deref(), Some("in_progress" | "cancelling"))
    {
        return Err(EvaluationSuperseded.into());
    }
    let attempt_id: Option<uuid::Uuid> = sqlx::query_scalar(
        "SELECT a.id FROM evaluation_attempts a
         WHERE a.commit_id = $1 AND a.attempt_number = $2 AND a.status = 'in_progress'
           AND NOT EXISTS (SELECT 1 FROM evaluation_attempts newer
               WHERE newer.commit_id = a.commit_id AND newer.attempt_number > a.attempt_number)
         FOR UPDATE OF a",
    )
    .bind(commit_id)
    .bind(expected_attempt)
    .fetch_optional(&mut *tx)
    .await?;
    let attempt_id = attempt_id.ok_or_else(|| anyhow::Error::new(EvaluationSuperseded))?;
    // Ownership precedes cancellation: an older worker cannot cancel a retry.
    if cancelled == Some(true) || status.as_deref() == Some("cancelling") {
        return Err(EvaluationCancelled.into());
    }
    sqlx::query(
        "INSERT INTO evaluation_attempt_inventory (evaluation_attempt_id, commit_id,
             attempt_number, configuration_names, selected_configuration_names, execution_mode)
         VALUES ($1, $2, $3, $4, $5, $6)
         ON CONFLICT (evaluation_attempt_id) DO NOTHING",
    )
    .bind(attempt_id)
    .bind(commit_id)
    .bind(expected_attempt)
    .bind(&discovered)
    .bind(&selected)
    .bind(mode.as_str())
    .execute(&mut *tx)
    .await?;
    // Compare after conflict without UPDATE: even a no-op UPDATE is forbidden.
    let identical: bool = sqlx::query_scalar(
        "SELECT commit_id = $2 AND attempt_number = $3 AND configuration_names = $4
             AND selected_configuration_names = $5 AND execution_mode = $6
         FROM evaluation_attempt_inventory WHERE evaluation_attempt_id = $1",
    )
    .bind(attempt_id)
    .bind(commit_id)
    .bind(expected_attempt)
    .bind(&discovered)
    .bind(&selected)
    .bind(mode.as_str())
    .fetch_one(&mut *tx)
    .await?;
    if !identical {
        return Err(InventoryConflict.into());
    }
    tx.commit().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use InventoryExecutionMode::{Mock, Native};

    fn names(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_string()).collect()
    }

    async fn active_commit(pool: &PgPool) -> i32 {
        let flake: i32 =
            sqlx::query_scalar("INSERT INTO flakes (name, repo_url) VALUES ($1, $2) RETURNING id")
                .bind(format!("inventory-{}", uuid::Uuid::new_v4()))
                .bind(format!(
                    "https://example.invalid/inventory/{}",
                    uuid::Uuid::new_v4()
                ))
                .fetch_one(pool)
                .await
                .unwrap();
        let commit: i32 = sqlx::query_scalar(
            "INSERT INTO commits (flake_id, git_commit_hash, commit_timestamp,
                 evaluation_status, evaluation_attempt_count, cancellation_requested)
             VALUES ($1, $2, NOW(), 'in_progress', 7, false) RETURNING id",
        )
        .bind(flake)
        .bind(uuid::Uuid::new_v4().to_string())
        .fetch_one(pool)
        .await
        .unwrap();
        // Commit insertion creates a queued attempt. Claim it as the fixture's
        // started attempt instead of adding a competing active attempt.
        sqlx::query(
            "UPDATE evaluation_attempts SET attempt_number = 7, status = 'in_progress',
                 started_at = NOW() WHERE commit_id = $1",
        )
        .bind(commit)
        .execute(pool)
        .await
        .unwrap();
        commit
    }

    async fn rows(pool: &PgPool, commit: i32) -> Vec<(i32, Vec<String>, Vec<String>, String)> {
        sqlx::query_as(
            "SELECT attempt_number, configuration_names, selected_configuration_names,
                 execution_mode FROM evaluation_attempt_inventory
             WHERE commit_id = $1 ORDER BY attempt_number",
        )
        .bind(commit)
        .fetch_all(pool)
        .await
        .unwrap()
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires isolated migration-backed PostgreSQL"]
    async fn inventory_normalizes_exact_names_and_rejects_replacement(pool: PgPool) {
        let commit = active_commit(&pool).await;
        let discovered = names(&["z", "A", "a", "z", " padded ", "é"]);
        let selected = names(&["z", "A", "z"]);
        persist_inventory(&pool, commit, 7, &discovered, &selected, Native)
            .await
            .unwrap();
        let before = rows(&pool, commit).await;
        assert_eq!(
            before,
            vec![(
                7,
                names(&[" padded ", "A", "a", "z", "é"]),
                names(&["A", "z"]),
                "native".into()
            )]
        );
        // Reordered and duplicate inputs denote the same immutable inventory.
        persist_inventory(
            &pool,
            commit,
            7,
            &names(&["é", "z", "a", "A", " padded "]),
            &names(&["A", "z"]),
            Native,
        )
        .await
        .unwrap();
        for (discovered, selected, mode) in [
            (names(&["A", "z"]), names(&["A", "z"]), Native),
            (discovered.clone(), names(&["A"]), Native),
            (discovered.clone(), selected.clone(), Mock),
        ] {
            let error = persist_inventory(&pool, commit, 7, &discovered, &selected, mode)
                .await
                .unwrap_err();
            assert!(error.is::<InventoryConflict>());
        }
        assert_eq!(rows(&pool, commit).await, before);
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires isolated migration-backed PostgreSQL"]
    async fn inventory_empty_and_selection_subset_are_distinct_from_missing(pool: PgPool) {
        let commit = active_commit(&pool).await;
        assert!(rows(&pool, commit).await.is_empty());
        assert!(
            persist_inventory(
                &pool,
                commit,
                7,
                &names(&[" exact "]),
                &names(&["exact"]),
                Native
            )
            .await
            .is_err()
        );
        assert!(rows(&pool, commit).await.is_empty());
        persist_inventory(&pool, commit, 7, &[], &[], Mock)
            .await
            .unwrap();
        persist_inventory(&pool, commit, 7, &[], &[], Mock)
            .await
            .unwrap();
        assert_eq!(
            rows(&pool, commit).await,
            vec![(7, vec![], vec![], "mock".into())]
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires isolated migration-backed PostgreSQL"]
    async fn inventory_schema_enforces_owner_immutability_and_parent_cascade(pool: PgPool) {
        let commit = active_commit(&pool).await;
        persist_inventory(
            &pool,
            commit,
            7,
            &names(&["one", "two"]),
            &names(&["two"]),
            Native,
        )
        .await
        .unwrap();
        for query in [
            "UPDATE evaluation_attempt_inventory SET configuration_names = configuration_names WHERE commit_id = $1",
            "DELETE FROM evaluation_attempt_inventory WHERE commit_id = $1",
        ] {
            assert!(
                sqlx::query(query)
                    .bind(commit)
                    .execute(&pool)
                    .await
                    .is_err()
            );
        }
        // Respect the global single-in-progress commit constraint while adding
        // an unrelated owner for the composite foreign-key checks.
        sqlx::query("UPDATE commits SET evaluation_status = 'complete' WHERE id = $1")
            .bind(commit)
            .execute(&pool)
            .await
            .unwrap();
        let foreign_commit = active_commit(&pool).await;
        let attempt_id: uuid::Uuid =
            sqlx::query_scalar("SELECT id FROM evaluation_attempts WHERE commit_id = $1")
                .bind(commit)
                .fetch_one(&pool)
                .await
                .unwrap();
        // Use the foreign attempt's UUID to avoid a primary-key conflict hiding
        // the composite ownership constraint under test.
        let foreign_id: uuid::Uuid =
            sqlx::query_scalar("SELECT id FROM evaluation_attempts WHERE commit_id = $1")
                .bind(foreign_commit)
                .fetch_one(&pool)
                .await
                .unwrap();
        for (id, owner, number, discovered, selected, mode) in [
            (foreign_id, commit, 7, "one", "one", "native"),
            (foreign_id, foreign_commit, 8, "one", "one", "native"),
            (foreign_id, foreign_commit, 7, "one", "two", "native"),
            (foreign_id, foreign_commit, 7, "one", "one", "unknown"),
        ] {
            assert!(sqlx::query("INSERT INTO evaluation_attempt_inventory (evaluation_attempt_id, commit_id, attempt_number, configuration_names, selected_configuration_names, execution_mode) VALUES ($1,$2,$3,$4,$5,$6)")
                .bind(id).bind(owner).bind(number).bind(names(&[discovered])).bind(names(&[selected])).bind(mode).execute(&pool).await.is_err());
        }
        sqlx::query("DELETE FROM evaluation_attempts WHERE id = $1")
            .bind(attempt_id)
            .execute(&pool)
            .await
            .unwrap();
        assert!(rows(&pool, commit).await.is_empty());
        persist_inventory(&pool, foreign_commit, 7, &[], &[], Mock)
            .await
            .unwrap();
        sqlx::query("DELETE FROM commits WHERE id = $1")
            .bind(foreign_commit)
            .execute(&pool)
            .await
            .unwrap();
        assert!(rows(&pool, foreign_commit).await.is_empty());
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires isolated migration-backed PostgreSQL"]
    async fn inventory_rejects_cancelled_superseded_and_nonactive_writers(pool: PgPool) {
        let commit = active_commit(&pool).await;
        for (status, attempt, cancelled, is_cancelled) in [
            ("in_progress", 7, true, true),
            ("cancelling", 7, false, true),
            ("cancelling", 8, true, false),
            ("complete", 7, false, false),
            ("pending", 7, false, false),
        ] {
            sqlx::query("UPDATE commits SET evaluation_status = $2, evaluation_attempt_count = $3, cancellation_requested = $4 WHERE id = $1")
                .bind(commit).bind(status).bind(attempt).bind(cancelled).execute(&pool).await.unwrap();
            let error = persist_inventory(&pool, commit, 7, &[], &[], Native)
                .await
                .unwrap_err();
            assert_eq!(error.is::<EvaluationCancelled>(), is_cancelled);
            assert_eq!(error.is::<EvaluationSuperseded>(), !is_cancelled);
        }
        sqlx::query("UPDATE commits SET evaluation_status = 'in_progress', cancellation_requested = false WHERE id = $1")
            .bind(commit).execute(&pool).await.unwrap();
        sqlx::query("UPDATE evaluation_attempts SET status = 'failed' WHERE commit_id = $1")
            .bind(commit)
            .execute(&pool)
            .await
            .unwrap();
        assert!(
            persist_inventory(&pool, commit, 7, &[], &[], Native)
                .await
                .unwrap_err()
                .is::<EvaluationSuperseded>()
        );
        assert!(
            persist_inventory(&pool, -1, 7, &[], &[], Native)
                .await
                .unwrap_err()
                .is::<EvaluationSuperseded>()
        );
        assert!(rows(&pool, commit).await.is_empty());
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires isolated migration-backed PostgreSQL"]
    async fn inventory_retry_never_inherits_or_accepts_older_writes(pool: PgPool) {
        let commit = active_commit(&pool).await;
        persist_inventory(&pool, commit, 7, &names(&["old"]), &[], Native)
            .await
            .unwrap();
        sqlx::query("UPDATE evaluation_attempts SET status = 'failed' WHERE commit_id = $1")
            .bind(commit)
            .execute(&pool)
            .await
            .unwrap();
        let retry_id: uuid::Uuid = sqlx::query_scalar("INSERT INTO evaluation_attempts (commit_id, attempt_number, status) VALUES ($1, 8, 'queued') RETURNING id")
            .bind(commit).fetch_one(&pool).await.unwrap();
        // Even an inconsistent active commit cannot authorize the older writer
        // once a newer queued attempt exists and the counter still reads seven.
        assert!(
            persist_inventory(&pool, commit, 7, &names(&["old"]), &[], Native)
                .await
                .unwrap_err()
                .is::<EvaluationSuperseded>()
        );
        assert!(
            persist_inventory(&pool, commit, 8, &[], &[], Mock)
                .await
                .unwrap_err()
                .is::<EvaluationSuperseded>()
        );
        let unavailable: bool = sqlx::query_scalar("SELECT NOT EXISTS (SELECT 1 FROM evaluation_attempt_inventory WHERE evaluation_attempt_id = $1)")
            .bind(retry_id).fetch_one(&pool).await.unwrap();
        assert!(unavailable);
        sqlx::query("UPDATE evaluation_attempts SET status = 'in_progress', started_at = NOW() WHERE id = $1")
            .bind(retry_id).execute(&pool).await.unwrap();
        sqlx::query("UPDATE commits SET evaluation_attempt_count = 8 WHERE id = $1")
            .bind(commit)
            .execute(&pool)
            .await
            .unwrap();
        persist_inventory(&pool, commit, 8, &names(&["new"]), &names(&["new"]), Mock)
            .await
            .unwrap();
        assert!(
            persist_inventory(&pool, commit, 7, &names(&["late"]), &[], Native)
                .await
                .unwrap_err()
                .is::<EvaluationSuperseded>()
        );
        assert_eq!(
            rows(&pool, commit).await,
            vec![
                (7, names(&["old"]), vec![], "native".into()),
                (8, names(&["new"]), names(&["new"]), "mock".into()),
            ]
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires isolated migration-backed PostgreSQL"]
    async fn inventory_competing_first_writes_preserve_one_complete_record(pool: PgPool) {
        let commit = active_commit(&pool).await;
        let left = names(&["left"]);
        let right = names(&["right"]);
        let (left_result, right_result) = tokio::join!(
            persist_inventory(&pool, commit, 7, &left, &left, Native),
            persist_inventory(&pool, commit, 7, &right, &right, Mock),
        );
        assert_ne!(left_result.is_ok(), right_result.is_ok());
        let expected = if left_result.is_ok() {
            assert!(right_result.unwrap_err().is::<InventoryConflict>());
            (7, left.clone(), left, "native".into())
        } else {
            assert!(left_result.unwrap_err().is::<InventoryConflict>());
            (7, right.clone(), right, "mock".into())
        };
        assert_eq!(rows(&pool, commit).await, vec![expected]);
    }
}
