//! First-write evaluation plans and consistent attempt-matched matrix reads.
//!
//! Public plans contain identities and whitelisted display fields only. They
//! never contain policy configuration, Nix expressions, overlays, or secrets.

use anyhow::{Context, Result, ensure};
use serde_json::Value;
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::api::eval_policy_matrix::*;
use crate::queries::commits::EvalPolicySystemRow;

mod scoped_read;

#[cfg(test)]
mod scoped_read_tests;

#[cfg(test)]
mod pinned_read_tests;

/// Locks current attempt ownership before resolving and capturing its plan.
///
/// # Errors
/// Returns an error for cancellation, supersession, duplicate capture, or SQL
/// failures. A duplicate must never execute newly resolved inputs.
pub(crate) async fn lock_attempt(
    tx: &mut Transaction<'_, Postgres>,
    commit_id: i32,
    number: i32,
) -> Result<Uuid> {
    let id = sqlx::query_scalar::<_, Uuid>(
        "SELECT a.id FROM commits c JOIN evaluation_attempts a ON a.commit_id = c.id
             AND a.attempt_number = c.evaluation_attempt_count
         WHERE c.id = $1 AND c.evaluation_attempt_count = $2
           AND c.evaluation_status = 'in_progress' AND NOT COALESCE(c.cancellation_requested, FALSE)
           AND a.status = 'in_progress' FOR SHARE OF c, a",
    )
    .bind(commit_id)
    .bind(number)
    .fetch_optional(&mut **tx)
    .await?
    .context("evaluation plan no longer owns active attempt")?;
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM evaluation_policy_plans WHERE commit_id = $1 AND attempt_number = $2)",
    )
    .bind(commit_id)
    .bind(number)
    .fetch_one(&mut **tx)
    .await?;
    ensure!(
        !exists,
        "evaluation attempt already has an immutable policy plan"
    );
    Ok(id)
}

/// Inserts a complete first-write plan through the loader's transaction.
///
/// # Errors
/// Returns an error for a missing attempt identity, duplicate plan, or SQL error.
/// It also rejects read-projected context, cells, details, or mock evidence.
pub(crate) async fn insert_plan(
    tx: &mut Transaction<'_, Postgres>,
    commit_id: i32,
    plan: &EvalPolicyMatrixSnapshot,
) -> Result<()> {
    let id = plan
        .evaluation_attempt_id
        .context("plan requires attempt identity")?;
    ensure!(
        plan.commit_sha.is_none()
            && plan.attempt_status.is_none()
            && plan.started_at.is_none()
            && plan.completed_at.is_none(),
        "capture payload cannot contain read-projected attempt context"
    );
    ensure!(
        !plan.mock_evidence,
        "capture payload cannot contain read-projected mock evidence"
    );
    ensure!(
        plan.rows
            .iter()
            .all(|row| row.cells.is_empty() && row.details.is_empty()),
        "capture payload cannot contain read-projected outcomes or details"
    );
    sqlx::query(
        "INSERT INTO evaluation_policy_plans (commit_id, attempt_number, evaluation_attempt_id, payload)
         VALUES ($1, $2, $3, $4)",
    )
    .bind(commit_id)
    .bind(plan.attempt_number)
    .bind(id)
    .bind(serde_json::to_value(plan)?)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// Reads legacy fields, captured provenance, and outcomes from one MVCC snapshot.
///
/// Returns the unrestricted administrator projection. HTTP callers MUST verify
/// administrator access or use [`fetch_matrix_for_user`] with the current user.
///
/// Exact-attempt isolated mock outcomes replace stale or markerless native
/// rows for the same configuration. If both sources have valid current-attempt
/// evidence, the configuration is unknown in structured and flat projections.
/// `mock_evidence` reports isolated records for this exact represented attempt,
/// even if merged evidence is ambiguous. It is independent of server mode and
/// recomputed at read time; stored plan metadata cannot set or clear this flag.
/// Confirmed pre-policy Nix failures expose their exact-attempt diagnostic in
/// the flat row, including when a prior build state is preserved. Such failures
/// do not establish individual control outcomes; structured cells stay unknown.
///
/// # Errors
/// Returns an error for a missing commit, SQL failure, or invalid persisted plan.
/// Missing plans produce explicit unavailable provenance without catalog reads.
///
/// # Examples
///
/// ```no_run
/// # async fn inspect(pool: &sqlx::PgPool, commit_id: i32) -> anyhow::Result<()> {
/// let (_, matrix) =
///     crystal_forge::queries::eval_policy_plans::fetch_matrix(pool, commit_id).await?;
/// assert!(matrix.rows.iter().all(|row| row.cells.len() == matrix.columns.len()));
/// # Ok(())
/// # }
/// ```
pub async fn fetch_matrix(
    pool: &PgPool,
    commit_id: i32,
) -> Result<(Vec<EvalPolicySystemRow>, EvalPolicyMatrixSnapshot)> {
    fetch_matrix_for_user(pool, commit_id, None)
        .await?
        .context("evaluation commit not found")
}

/// Returns a matrix restricted to the caller's current system environments.
///
/// `None` user scope is reserved for authenticated administrators. A missing or
/// inaccessible commit returns `None` without distinguishing the two cases.
/// Captured system UUIDs must still exist on the commit's flake with the same
/// configuration and a current environment membership. Historical assignments
/// never grant access. Authorization and evidence share one repeatable-read
/// snapshot, including membership revocation and system moves.
///
/// # Errors
/// Returns SQL errors or invalid persisted plan errors for accessible commits.
///
/// # Examples
///
/// ```no_run
/// # async fn inspect(pool: &sqlx::PgPool, user: uuid::Uuid) -> anyhow::Result<()> {
/// let visible = crystal_forge::queries::eval_policy_plans::fetch_matrix_for_user(
///     pool, 42, Some(user),
/// ).await?;
/// if let Some((_, matrix)) = visible {
///     assert!(matrix.rows.iter().all(|row| row.system_id.is_some()));
/// }
/// # Ok(())
/// # }
/// ```
pub async fn fetch_matrix_for_user(
    pool: &PgPool,
    commit_id: i32,
    user_id: Option<Uuid>,
) -> Result<Option<(Vec<EvalPolicySystemRow>, EvalPolicyMatrixSnapshot)>> {
    fetch_matrix_selected(pool, commit_id, user_id, None).await
}

/// Returns the caller's current-environment projection of one exact attempt.
///
/// The UUID must belong to `commit_id`. Missing, foreign, and inaccessible
/// selections return None without falling back to the latest attempt. None user
/// scope is reserved for administrators and unrestricted internal callers.
/// Authorization, context, metadata, and evidence share one repeatable read.
/// Mutable native outcomes no longer marked for this attempt remain unknown;
/// isolated mock outcomes are retained under their exact attempt UUID.
///
/// # Errors
/// Returns SQL errors or invalid persisted plan errors for accessible commits.
///
/// # Examples
/// ```no_run
/// # async fn inspect(pool: &sqlx::PgPool, user: uuid::Uuid, attempt: uuid::Uuid) -> anyhow::Result<()> {
/// let selected = crystal_forge::queries::eval_policy_plans::fetch_matrix_for_user_at_attempt(
///     pool, 42, Some(user), attempt,
/// ).await?;
/// assert!(selected.is_none_or(|(_, matrix)| matrix.evaluation_attempt_id == Some(attempt)));
/// # Ok(())
/// # }
/// ```
pub async fn fetch_matrix_for_user_at_attempt(
    pool: &PgPool,
    commit_id: i32,
    user_id: Option<Uuid>,
    attempt_id: Uuid,
) -> Result<Option<(Vec<EvalPolicySystemRow>, EvalPolicyMatrixSnapshot)>> {
    fetch_matrix_selected(pool, commit_id, user_id, Some(attempt_id)).await
}

async fn fetch_matrix_selected(
    pool: &PgPool,
    commit_id: i32,
    user_id: Option<Uuid>,
    pin: Option<Uuid>,
) -> Result<Option<(Vec<EvalPolicySystemRow>, EvalPolicyMatrixSnapshot)>> {
    let mut tx = pool.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
        .execute(&mut *tx)
        .await?;
    let Some(scope) = scoped_read::authorize(&mut tx, commit_id, user_id).await? else {
        tx.commit().await?;
        return Ok(None);
    };
    let context = sqlx::query_as::<_, (i32, Option<Uuid>, String, Option<String>, Option<chrono::DateTime<chrono::Utc>>, Option<chrono::DateTime<chrono::Utc>>)>(
        "SELECT COALESCE(a.attempt_number, c.evaluation_attempt_count, 0), a.id,
             c.git_commit_hash,
             CASE WHEN a.attempt_number = (SELECT MAX(latest.attempt_number)
                        FROM evaluation_attempts latest WHERE latest.commit_id = c.id)
                        AND a.attempt_number = c.evaluation_attempt_count
                        AND a.status = 'in_progress'
                        AND (c.evaluation_status = 'cancelling' OR
                            (c.evaluation_status = 'in_progress' AND COALESCE(c.cancellation_requested, FALSE)))
                  THEN 'cancelling' ELSE a.status END,
             a.started_at, a.completed_at FROM commits c
         LEFT JOIN evaluation_attempts a ON a.commit_id = c.id
              AND (($2::uuid IS NOT NULL AND a.id = $2)
                   OR ($2::uuid IS NULL AND a.attempt_number = (SELECT MAX(latest.attempt_number)
                       FROM evaluation_attempts latest WHERE latest.commit_id = c.id)))
         WHERE c.id = $1 AND ($2::uuid IS NULL OR a.id IS NOT NULL)",
    )
    .bind(commit_id)
    .bind(pin)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(context) = context else {
        tx.commit().await?;
        return Ok(None);
    };
    let attempt = (context.0, context.1);
    let payload = sqlx::query_scalar::<_, Value>(
        "SELECT payload FROM evaluation_policy_plans
         WHERE commit_id = $1 AND attempt_number = $2 AND evaluation_attempt_id = $3",
    )
    .bind(commit_id)
    .bind(attempt.0)
    .bind(attempt.1)
    .fetch_optional(&mut *tx)
    .await?;
    // Status IDs mirror EvaluationStatus: a completed Nix evaluation or known
    // downstream build state can supply evidence. Pending, NULL, and unknown
    // states cannot prove evaluation completion even with a matching marker.
    let mut rows = sqlx::query_as::<_, EvalPolicySystemRow>(
        "SELECT d.derivation_name AS system_name,
                CASE WHEN d.status_id = 6 THEN 'eval_failed'
                     WHEN d.status_id IN (5, 7, 8, 10, 12) THEN 'evaluated'
                     ELSE 'unavailable' END AS eval_status,
                d.error_message, d.policy_results, d.policy_requirements_met
         FROM derivations d WHERE d.commit_id = $1 AND d.derivation_type = 'nixos'
         ORDER BY d.derivation_name",
    )
    .bind(commit_id)
    .fetch_all(&mut *tx)
    .await?;
    for row in &mut rows {
        project_native_failure(row, attempt.0);
    }
    let mock_rows = sqlx::query_as::<_, EvalPolicySystemRow>(
        "SELECT configuration_name AS system_name, 'evaluated'::text AS eval_status,
                NULL::text AS error_message, policy_results, policy_requirements_met
         FROM evaluation_mock_outcomes
         WHERE commit_id = $1 AND attempt_number = $2 AND evaluation_attempt_id = $3
         ORDER BY configuration_name",
    )
    .bind(commit_id)
    .bind(attempt.0)
    .bind(attempt.1)
    .fetch_all(&mut *tx)
    .await?;
    let mock_configurations: std::collections::HashSet<_> = mock_rows
        .iter()
        .map(|row| row.system_name.clone())
        .collect();
    rows = merge_mock_outcomes(rows, mock_rows, attempt.0);
    let mut plan = match payload {
        Some(value) => {
            let plan: EvalPolicyMatrixSnapshot = serde_json::from_value(value)?;
            ensure!(
                plan.attempt_number == attempt.0 && plan.evaluation_attempt_id == attempt.1,
                "persisted policy plan attempt mismatch"
            );
            plan
        }
        None => legacy_snapshot(attempt.0, attempt.1, &rows),
    };
    // Context comes only from the selected commit/attempt in this transaction.
    // Never copy the commit-wide error: it can describe a hidden environment.
    plan.commit_sha = Some(context.2);
    plan.attempt_status = context.3;
    plan.started_at = context.4;
    plan.completed_at = context.5;
    // This transient flag describes durable outcome evidence, not the loader's
    // pre-execution plan. Never write the projected snapshot back to storage.
    if let Some(scope) = &scope {
        scoped_read::project(scope, &mut plan, &mut rows);
    }
    plan.mock_evidence = if scope.is_none() {
        !mock_configurations.is_empty()
    } else {
        plan.rows
            .iter()
            .any(|row| mock_configurations.contains(&row.configuration_name))
    };
    project_outcomes(&mut plan, &rows);
    if scope.is_some() {
        scoped_read::project_flat(&plan, &mut rows);
    }
    // COMPATIBILITY: Flat fields remain available, but must not display
    // another attempt's mutable derivation evidence alongside this plan.
    // Markerless legacy documents retain their existing latest projection;
    // pinned reads cannot attribute those documents to the selected UUID.
    for row in &mut rows {
        if pin.is_some()
            || plan.provenance == EvalMatrixProvenance::Captured
            || row.policy_results.get("evaluation_attempt").is_some()
        {
            if row
                .policy_results
                .get("evaluation_attempt")
                .and_then(Value::as_i64)
                != Some(i64::from(plan.attempt_number))
                || !matches!(row.eval_status.as_str(), "evaluated" | "eval_failed")
            {
                row.policy_results = serde_json::json!({});
                row.policy_requirements_met = None;
                row.eval_status = "evaluated".into();
                row.error_message = None;
            }
        }
    }
    tx.commit().await?;
    Ok(Some((rows, plan)))
}

// A preserved build status describes an irreversible artifact, not the latest
// evaluation. Only the finalizer's complete, exact-attempt failure document can
// override that status. Never fall back to a previous build's error message.
fn project_native_failure(row: &mut EvalPolicySystemRow, attempt: i32) {
    if row.policy_results.get("evaluation_failure").is_none()
        && row.policy_results.get("evaluation_failure_error").is_none()
    {
        return;
    }
    let error = row
        .policy_results
        .get("evaluation_failure_error")
        .and_then(Value::as_str)
        .filter(|error| !error.trim().is_empty());
    if row
        .policy_results
        .get("evaluation_attempt")
        .and_then(Value::as_i64)
        == Some(i64::from(attempt))
        && row
            .policy_results
            .get("evaluation_failure")
            .and_then(Value::as_bool)
            == Some(true)
        && let Some(error) = error
    {
        row.eval_status = "eval_failed".into();
        row.error_message = Some(error.to_owned());
        row.policy_requirements_met = Some(false);
    } else {
        row.eval_status = "unavailable".into();
        row.error_message = None;
        row.policy_requirements_met = None;
        row.policy_results = serde_json::json!({});
    }
}

// Only exact-attempt mock rows reach this function. A stale native carrier must
// not hide a current isolated outcome; two valid carriers cannot be reconciled
// by source priority. Collapse ambiguity to one evidence-free compatibility row
// so the flat agent fields cannot accidentally pick a passing duplicate either.
fn merge_mock_outcomes(
    mut native: Vec<EvalPolicySystemRow>,
    mock: Vec<EvalPolicySystemRow>,
    attempt: i32,
) -> Vec<EvalPolicySystemRow> {
    for mut row in mock {
        let ambiguous = native.iter().any(|other| {
            other.system_name == row.system_name
                && matches!(other.eval_status.as_str(), "evaluated" | "eval_failed")
                && other
                    .policy_results
                    .get("evaluation_attempt")
                    .and_then(Value::as_i64)
                    == Some(i64::from(attempt))
        });
        native.retain(|other| other.system_name != row.system_name);
        if ambiguous {
            row.policy_results = serde_json::json!({});
            row.policy_requirements_met = None;
            row.eval_status = "unavailable".into();
        }
        native.push(row);
    }
    native.sort_by(|a, b| a.system_name.cmp(&b.system_name));
    native
}

fn legacy_snapshot(
    number: i32,
    id: Option<Uuid>,
    rows: &[EvalPolicySystemRow],
) -> EvalPolicyMatrixSnapshot {
    EvalPolicyMatrixSnapshot {
        evaluation_attempt_id: id,
        attempt_number: number,
        commit_sha: None,
        attempt_status: None,
        started_at: None,
        completed_at: None,
        mock_evidence: false,
        provenance: EvalMatrixProvenance::LegacyUnavailable,
        columns: Vec::new(),
        bundles: Vec::new(),
        empty_bundle_metadata_complete: false,
        rows: rows
            .iter()
            .map(|r| EvalMatrixRow {
                greenfield: None,
                system_id: None,
                system_name: r.system_name.clone(),
                configuration_name: r.system_name.clone(),
                resolution: EvalMatrixResolution::Unavailable,
                evaluation_digest: None,
                assigned_bundles: Vec::new(),
                policies: Vec::new(),
                cells: Vec::new(),
                details: Vec::new(),
            })
            .collect(),
    }
}

fn project_outcomes(plan: &mut EvalPolicyMatrixSnapshot, outcomes: &[EvalPolicySystemRow]) {
    for row in &mut plan.rows {
        // Do not choose arbitrarily if malformed data contains duplicate configs.
        let mut matching = outcomes
            .iter()
            .filter(|r| r.system_name == row.configuration_name);
        let outcome = matching.next();
        let outcome = if matching.next().is_none() {
            outcome
        } else {
            None
        };
        let (cells, details) = plan
            .columns
            .iter()
            .map(|column| {
                let binding = row
                    .policies
                    .iter()
                    .find(|p| p.policy_version_id == column.policy_version_id);
                let cell = classify_cell(row.resolution, binding, outcome, plan.attempt_number);
                // Evidence must pass the cell's ownership and availability
                // gates. Copy only the detail string, not raw metadata.
                let detail = if matches!(
                    cell,
                    EvalMatrixCell::Pass | EvalMatrixCell::Warn | EvalMatrixCell::Fail
                ) {
                    binding
                        .and_then(|binding| {
                            matching_policy_result(binding, outcome, plan.attempt_number)
                        })
                        .and_then(|result| result.get("details").and_then(Value::as_str))
                        .map(str::to_owned)
                } else {
                    None
                };
                (cell, detail)
            })
            .unzip();
        row.cells = cells;
        row.details = details;
    }
}

// Mutable derivation rows are not attempt-owned. Both statuses and evidence
// must use the represented attempt's exact UUID-keyed policy result.
fn matching_policy_result<'a>(
    binding: &EvalPolicyBinding,
    outcome: Option<&'a EvalPolicySystemRow>,
    attempt: i32,
) -> Option<&'a Value> {
    let outcome = outcome?;
    if !binding.executable
        || outcome
            .policy_results
            .get("evaluation_attempt")
            .and_then(Value::as_i64)
            != Some(i64::from(attempt))
        || outcome.eval_status != "evaluated"
    {
        return None;
    }
    let key = binding.policy_version_id.to_string();
    outcome.policy_results.get("assigned")?.get(&key)
}

fn classify_cell(
    resolution: EvalMatrixResolution,
    binding: Option<&EvalPolicyBinding>,
    outcome: Option<&EvalPolicySystemRow>,
    attempt: i32,
) -> EvalMatrixCell {
    if resolution != EvalMatrixResolution::Resolved {
        return EvalMatrixCell::Unknown;
    }
    let Some(binding) = binding else {
        return EvalMatrixCell::NotAssigned;
    };
    let Some(result) = matching_policy_result(binding, outcome, attempt) else {
        return EvalMatrixCell::Unknown;
    };
    let passed = result.get("passed").and_then(Value::as_bool);
    let blocking = result.get("blocking").and_then(Value::as_bool);
    if result.get("blocking").is_some() && blocking.is_none() {
        return EvalMatrixCell::Unknown;
    }
    match (passed, blocking) {
        (Some(true), Some(true)) => EvalMatrixCell::Unknown,
        (Some(true), _) => EvalMatrixCell::Pass,
        (Some(false), Some(false)) => EvalMatrixCell::Warn,
        (Some(false), Some(true)) if !binding.report_only => EvalMatrixCell::Fail,
        (Some(false), None) if binding.report_only => EvalMatrixCell::Warn,
        (Some(false), None) => match result.get("strict").and_then(Value::as_bool) {
            Some(true) => EvalMatrixCell::Fail,
            Some(false) => EvalMatrixCell::Warn,
            None => EvalMatrixCell::Unknown,
        },
        _ => EvalMatrixCell::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn binding(id: Uuid, report_only: bool) -> EvalPolicyBinding {
        EvalPolicyBinding {
            policy_version_id: id,
            report_only,
            executable: true,
            memberships: vec![],
        }
    }

    fn outcome(id: Uuid, attempt: Value, result: Value) -> EvalPolicySystemRow {
        EvalPolicySystemRow {
            system_name: "shared".into(),
            eval_status: "evaluated".into(),
            error_message: None,
            policy_requirements_met: Some(true),
            policy_results: json!({
                "evaluation_attempt": attempt,
                "assigned": {id.to_string(): result}
            }),
        }
    }

    #[test]
    fn mock_merge_replaces_stale_native_but_rejects_dual_current_evidence() {
        let mut snapshot = plan();
        let id = snapshot.columns[0].policy_version_id;
        let mock = || {
            outcome(
                id,
                json!(2),
                json!({"passed": true, "details": "[MOCK] pass"}),
            )
        };
        for marker in [Value::Null, json!(1), json!("2")] {
            let merged = merge_mock_outcomes(
                vec![outcome(id, marker, json!({"passed": false}))],
                vec![mock()],
                2,
            );
            assert_eq!(merged.len(), 1);
            project_outcomes(&mut snapshot, &merged);
            assert_eq!(snapshot.rows[0].cells, vec![EvalMatrixCell::Pass]);
            assert_eq!(snapshot.rows[1].cells, vec![EvalMatrixCell::Unknown]);
        }
        let merged = merge_mock_outcomes(
            vec![outcome(id, json!(2), json!({"passed": true}))],
            vec![mock()],
            2,
        );
        assert_eq!(merged.len(), 1);
        assert!(merged[0].policy_requirements_met.is_none());
        assert!(merged[0].policy_results.get("assigned").is_none());
        project_outcomes(&mut snapshot, &merged);
        assert_eq!(snapshot.rows[0].cells, vec![EvalMatrixCell::Unknown]);
        assert_eq!(snapshot.rows[0].details, vec![None]);
    }

    #[test]
    fn applicable_missing_stale_and_invalid_outcomes_are_unknown() {
        let id = Uuid::new_v4();
        let binding = binding(id, false);
        let classify = |row: Option<&EvalPolicySystemRow>| {
            classify_cell(EvalMatrixResolution::Resolved, Some(&binding), row, 2)
        };
        assert_eq!(classify(None), EvalMatrixCell::Unknown);
        for marker in [Value::Null, json!(1), json!("2"), json!(3)] {
            assert_eq!(
                classify(Some(&outcome(id, marker, json!({"passed": true})))),
                EvalMatrixCell::Unknown
            );
        }
        for result in [
            json!({}),
            json!({"passed": "true"}),
            json!({"passed": true, "blocking": true}),
            json!({"passed": true, "blocking": "false"}),
        ] {
            assert_eq!(
                classify(Some(&outcome(id, json!(2), result))),
                EvalMatrixCell::Unknown
            );
        }
        let absent = outcome(Uuid::new_v4(), json!(2), json!({"passed": true}));
        assert_eq!(classify(Some(&absent)), EvalMatrixCell::Unknown);
    }

    #[test]
    fn invalid_statuses_and_blocking_metadata_remain_unknown() {
        let mut snapshot = plan();
        let id = snapshot.columns[0].policy_version_id;
        for status in ["", "pending", "unrecognized", "eval_failed"] {
            let mut row = outcome(
                id,
                json!(2),
                json!({"passed": true, "details": "unavailable"}),
            );
            row.eval_status = status.to_string();
            project_outcomes(&mut snapshot, &[row]);
            assert_eq!(snapshot.rows[0].cells, vec![EvalMatrixCell::Unknown]);
            assert_eq!(snapshot.rows[0].details, vec![None]);
        }
        let binding = binding(id, false);
        for (result, expected) in [
            (json!({"passed": true}), EvalMatrixCell::Pass),
            (
                json!({"passed": true, "strict": false}),
                EvalMatrixCell::Pass,
            ),
            (
                json!({"passed": true, "strict": true}),
                EvalMatrixCell::Pass,
            ),
            (
                json!({"passed": true, "blocking": false}),
                EvalMatrixCell::Pass,
            ),
            (
                json!({"passed": true, "blocking": true}),
                EvalMatrixCell::Unknown,
            ),
            (
                json!({"passed": false, "blocking": false}),
                EvalMatrixCell::Warn,
            ),
            (
                json!({"passed": false, "blocking": true}),
                EvalMatrixCell::Fail,
            ),
            (
                json!({"passed": false, "strict": false}),
                EvalMatrixCell::Warn,
            ),
            (
                json!({"passed": false, "strict": true}),
                EvalMatrixCell::Fail,
            ),
            (json!({"passed": false}), EvalMatrixCell::Unknown),
            (
                json!({"passed": false, "blocking": null}),
                EvalMatrixCell::Unknown,
            ),
            (
                json!({"passed": false, "blocking": "false"}),
                EvalMatrixCell::Unknown,
            ),
            (json!({"blocking": false}), EvalMatrixCell::Unknown),
            (json!({"blocking": true}), EvalMatrixCell::Unknown),
        ] {
            assert_eq!(
                classify_cell(
                    EvalMatrixResolution::Resolved,
                    Some(&binding),
                    Some(&outcome(id, json!(2), result)),
                    2,
                ),
                expected
            );
        }
    }

    #[test]
    fn only_complete_resolution_establishes_nonapplicability() {
        assert_eq!(
            classify_cell(EvalMatrixResolution::Resolved, None, None, 2),
            EvalMatrixCell::NotAssigned
        );
        for resolution in [
            EvalMatrixResolution::Conflict,
            EvalMatrixResolution::Unavailable,
            EvalMatrixResolution::Skipped,
        ] {
            assert_eq!(
                classify_cell(resolution, None, None, 2),
                EvalMatrixCell::Unknown
            );
        }
    }

    #[test]
    fn report_only_failure_is_warning_and_skipped_checks_are_unknown() {
        let id = Uuid::new_v4();
        let mut binding = binding(id, true);
        let failed = outcome(
            id,
            json!(2),
            json!({"passed": false, "strict": true, "blocking": false}),
        );
        assert_eq!(
            classify_cell(
                EvalMatrixResolution::Resolved,
                Some(&binding),
                Some(&failed),
                2
            ),
            EvalMatrixCell::Warn
        );
        binding.executable = false;
        let passed = outcome(id, json!(2), json!({"passed": true}));
        assert_eq!(
            classify_cell(
                EvalMatrixResolution::Resolved,
                Some(&binding),
                Some(&passed),
                2
            ),
            EvalMatrixCell::Unknown
        );
        binding.executable = true;
        binding.report_only = false;
        let blocking = outcome(id, json!(2), json!({"passed": false, "blocking": true}));
        assert_eq!(
            classify_cell(
                EvalMatrixResolution::Resolved,
                Some(&binding),
                Some(&blocking),
                2
            ),
            EvalMatrixCell::Fail
        );
        assert_eq!(
            classify_cell(
                EvalMatrixResolution::Resolved,
                Some(&binding),
                Some(&passed),
                2
            ),
            EvalMatrixCell::Pass
        );
        binding.report_only = true;
        assert_eq!(
            classify_cell(
                EvalMatrixResolution::Resolved,
                Some(&binding),
                Some(&blocking),
                2
            ),
            EvalMatrixCell::Unknown
        );
    }

    pub(super) fn plan() -> EvalPolicyMatrixSnapshot {
        let policy = Uuid::new_v4();
        let bundles = [Uuid::new_v4(), Uuid::new_v4()];
        let mut binding = binding(policy, false);
        binding.memberships = bundles
            .iter()
            .map(|id| EvalBundleMembership {
                bundle_version_id: *id,
                assignment_id: Some(Uuid::new_v4()),
                assignment_version_id: Some(Uuid::new_v4()),
                source: EvalBundleSource::Assignment,
            })
            .collect();
        EvalPolicyMatrixSnapshot {
            evaluation_attempt_id: Some(Uuid::new_v4()),
            attempt_number: 2,
            commit_sha: None,
            attempt_status: None,
            started_at: None,
            completed_at: None,
            mock_evidence: false,
            provenance: EvalMatrixProvenance::Captured,
            empty_bundle_metadata_complete: false,
            columns: vec![EvalPolicyColumn {
                policy_version_id: policy,
                policy_id: Uuid::new_v4(),
                name: "Version-owned name".into(),
                version: "1".into(),
                identifiers: Vec::new(),
                description: None,
                assertion: None,
                attribute_path: None,
            }],
            bundles: bundles
                .iter()
                .map(|id| EvalBundleColumn {
                    bundle_version_id: *id,
                    bundle_id: Uuid::new_v4(),
                    name: "Captured bundle".into(),
                    version: "1".into(),
                    framework: "Captured framework".into(),
                    framework_version: Some("2026".into()),
                })
                .collect(),
            rows: ["shared", "no-result"]
                .iter()
                .map(|name| EvalMatrixRow {
                    greenfield: None,
                    system_id: Some(Uuid::new_v4()),
                    system_name: format!("host-{name}"),
                    configuration_name: (*name).into(),
                    resolution: EvalMatrixResolution::Resolved,
                    evaluation_digest: Some("captured-input-digest".into()),
                    assigned_bundles: binding.memberships.clone(),
                    policies: vec![binding.clone()],
                    cells: vec![],
                    details: vec![],
                })
                .collect(),
        }
    }

    #[test]
    fn earlier_capture_json_defaults_contexts_and_preserves_assignment_ids() {
        let original = plan();
        let mut value = serde_json::to_value(&original).unwrap();
        value.as_object_mut().unwrap().remove("mock_evidence");
        for row in value["rows"].as_array_mut().unwrap() {
            row.as_object_mut().unwrap().remove("assigned_bundles");
            row.as_object_mut().unwrap().remove("details");
            for policy in row["policies"].as_array_mut().unwrap() {
                for membership in policy["memberships"].as_array_mut().unwrap() {
                    membership.as_object_mut().unwrap().remove("source");
                }
            }
        }
        let decoded: EvalPolicyMatrixSnapshot = serde_json::from_value(value).unwrap();
        assert!(!decoded.mock_evidence);
        assert!(!decoded.empty_bundle_metadata_complete);
        assert!(decoded.rows.iter().all(|r| r.assigned_bundles.is_empty()));
        assert!(decoded.rows.iter().all(|r| r.details.is_empty()));
        let membership = &decoded.rows[0].policies[0].memberships[0];
        assert_eq!(membership.source, EvalBundleSource::Assignment);
        assert_eq!(
            membership.assignment_id,
            original.rows[0].policies[0].memberships[0].assignment_id
        );
        assert!(membership.assignment_version_id.is_some());
    }

    #[test]
    fn mock_evidence_serializes_without_changing_capture_defaults() {
        let captured = plan();
        assert!(!captured.mock_evidence);
        let mut projected = captured.clone();
        projected.mock_evidence = true;
        let value = serde_json::to_value(&projected).unwrap();
        assert_eq!(value["mock_evidence"], true);
        assert!(
            serde_json::from_value::<EvalPolicyMatrixSnapshot>(value)
                .unwrap()
                .mock_evidence
        );
        assert!(!captured.mock_evidence);
        assert!(!legacy_snapshot(2, None, &[]).mock_evidence);
    }

    #[test]
    fn details_are_uuid_bound_attempt_matched_and_transient() {
        let mut snapshot = plan();
        let original = serde_json::to_value(&snapshot).unwrap();
        let id = snapshot.columns[0].policy_version_id;
        let evidence = "Missing required packages: grafana\nActual evaluator evidence.";
        let failed = json!({
            "passed": false, "blocking": true, "details": evidence,
            "config": {"expression": "must not be copied"}
        });
        for marker in [Value::Null, json!(1), json!("2"), json!(3)] {
            project_outcomes(&mut snapshot, &[outcome(id, marker, failed.clone())]);
            assert_eq!(snapshot.rows[0].cells, vec![EvalMatrixCell::Unknown]);
            assert_eq!(snapshot.rows[0].details, vec![None]);
        }
        project_outcomes(&mut snapshot, &[outcome(id, json!(2), failed.clone())]);
        assert_eq!(snapshot.rows[0].cells, vec![EvalMatrixCell::Fail]);
        assert_eq!(snapshot.rows[0].details, vec![Some(evidence.to_string())]);
        assert_eq!(snapshot.rows[1].details, vec![None]);
        assert!(
            !serde_json::to_string(&snapshot)
                .unwrap()
                .contains("must not be copied")
        );
        // Read projection mutates only its returned clone, not stored plan JSON.
        let stored: EvalPolicyMatrixSnapshot = serde_json::from_value(original).unwrap();
        assert!(
            stored
                .rows
                .iter()
                .all(|row| row.details.is_empty() && row.cells.is_empty())
        );

        let other_id = Uuid::new_v4();
        snapshot.columns.push(EvalPolicyColumn {
            policy_version_id: other_id,
            ..snapshot.columns[0].clone()
        });
        project_outcomes(&mut snapshot, &[outcome(id, json!(2), failed.clone())]);
        assert_eq!(
            snapshot.rows[0].cells,
            vec![EvalMatrixCell::Fail, EvalMatrixCell::NotAssigned]
        );
        assert_eq!(
            snapshot.rows[0].details,
            vec![Some(evidence.to_string()), None]
        );
        project_outcomes(
            &mut snapshot,
            &[outcome(other_id, json!(2), failed.clone())],
        );
        assert_eq!(snapshot.rows[0].details, vec![None, None]);
        for resolution in [
            EvalMatrixResolution::Unavailable,
            EvalMatrixResolution::Skipped,
            EvalMatrixResolution::Conflict,
        ] {
            snapshot.rows[0].resolution = resolution;
            project_outcomes(&mut snapshot, &[outcome(id, json!(2), failed.clone())]);
            assert_eq!(snapshot.rows[0].details, vec![None, None]);
        }
        snapshot.rows[0].resolution = EvalMatrixResolution::Resolved;
        snapshot.rows[0].policies[0].executable = false;
        project_outcomes(&mut snapshot, &[outcome(id, json!(2), failed.clone())]);
        assert_eq!(snapshot.rows[0].details, vec![None, None]);
        snapshot.rows[0].policies[0].executable = true;
        for result in [
            json!({"passed": false, "blocking": true, "details": 123}),
            json!({"passed": "false", "details": evidence}),
        ] {
            project_outcomes(&mut snapshot, &[outcome(id, json!(2), result)]);
            assert_eq!(snapshot.rows[0].details, vec![None, None]);
        }
    }

    #[test]
    fn partial_planned_rows_and_overlap_survive_projection_without_duplication() {
        let mut plan = plan();
        let id = plan.columns[0].policy_version_id;
        project_outcomes(&mut plan, &[outcome(id, json!(2), json!({"passed": true}))]);
        assert_eq!(plan.rows.len(), 2);
        assert_eq!(plan.columns.len(), 1);
        assert_eq!(plan.rows[0].policies[0].memberships.len(), 2);
        assert_eq!(plan.rows[0].cells, vec![EvalMatrixCell::Pass]);
        assert_eq!(plan.rows[1].cells, vec![EvalMatrixCell::Unknown]);
    }

    #[test]
    fn legacy_results_never_infer_bundle_or_policy_memberships() {
        let row = outcome(Uuid::new_v4(), json!(2), json!({"passed": true}));
        let plan = legacy_snapshot(2, None, &[row]);
        assert_eq!(plan.provenance, EvalMatrixProvenance::LegacyUnavailable);
        assert!(plan.columns.is_empty() && plan.bundles.is_empty());
        assert_eq!(plan.rows[0].resolution, EvalMatrixResolution::Unavailable);
        assert!(plan.rows[0].policies.is_empty());
    }

    pub(super) async fn commit(pool: &PgPool) -> (i32, Uuid, i32) {
        let flake: i32 =
            sqlx::query_scalar("INSERT INTO flakes (name, repo_url) VALUES ($1, $2) RETURNING id")
                .bind(Uuid::new_v4().to_string())
                .bind(format!("https://example.invalid/plan/{}", Uuid::new_v4()))
                .fetch_one(pool)
                .await
                .unwrap();
        let commit: i32 = sqlx::query_scalar("INSERT INTO commits (flake_id, git_commit_hash, commit_timestamp) VALUES ($1, $2, NOW()) RETURNING id")
            .bind(flake)
            .bind(Uuid::new_v4().to_string())
            .fetch_one(pool)
            .await
            .unwrap();
        let number = match crate::queries::commits::mark_commit_evaluation_started(pool, commit)
            .await
            .unwrap()
        {
            crate::queries::commits::EvalStartOutcome::Started { attempt } => attempt,
            other => panic!("fixture must start: {other:?}"),
        };
        let id = sqlx::query_scalar(
            "SELECT id FROM evaluation_attempts WHERE commit_id = $1 AND attempt_number = $2",
        )
        .bind(commit)
        .bind(number)
        .fetch_one(pool)
        .await
        .unwrap();
        (commit, id, number)
    }

    fn confirmed_failure_plan() -> crate::models::evaluate_with_policies::EvaluationPlan {
        use crate::models::evaluate_with_policies::{ConfirmedSystemFailure, EvaluationPlan};
        EvaluationPlan {
            results: vec![],
            policy_checks: vec![],
            successful_systems: vec![],
            confirmed_failures: vec![ConfirmedSystemFailure {
                system_name: "shared".into(),
                derivation_target: "https://example.invalid/plan#nixosConfigurations.shared".into(),
                error: "actual Nix module evaluation failure".into(),
            }],
            evaluation_snapshots: Default::default(),
            snapshot_capture_failures: Default::default(),
            flake_output_snapshot: None,
            had_system_eval_errors: true,
            force_build_job_insert_failure: false,
        }
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires authoritative server-regressions sandbox PostgreSQL"]
    async fn confirmed_native_finalizer_failure_is_attempt_owned_and_preserves_build(pool: PgPool) {
        use crate::models::evaluate_with_policies::{
            EvaluationFinalizeOutcome, finalize_evaluation_attempt,
        };
        // Exercise inserted, resettable evaluation, and irreversible build rows
        // through the production finalizer rather than manufacturing its stamp.
        for status in [None, Some(3), Some(10)] {
            let (commit, attempt_id, number) = commit(&pool).await;
            let mut captured = plan();
            captured.evaluation_attempt_id = Some(attempt_id);
            captured.attempt_number = number;
            let mut tx = pool.begin().await.unwrap();
            insert_plan(&mut tx, commit, &captured).await.unwrap();
            tx.commit().await.unwrap();
            if let Some(status) = status {
                sqlx::query(
                    "INSERT INTO derivations (commit_id, derivation_type, derivation_name,
                     status_id, derivation_path, store_path, expected_store_path, error_message)
                     VALUES ($1, 'nixos', 'shared', $2, $3, $4, $4, 'old build diagnostic')",
                )
                .bind(commit)
                .bind(status)
                .bind(format!("/nix/store/test-{commit}.drv"))
                .bind(format!("/nix/store/test-{commit}"))
                .execute(&pool)
                .await
                .unwrap();
            }
            let evaluation = confirmed_failure_plan();
            assert!(matches!(
                finalize_evaluation_attempt(&pool, commit, number, &evaluation)
                    .await
                    .unwrap(),
                EvaluationFinalizeOutcome::Completed { .. }
            ));
            let stored: (i32, Option<String>, Option<String>, Option<String>, Value) =
                sqlx::query_as(
                    "SELECT status_id, derivation_path, store_path, expected_store_path,
                     policy_results FROM derivations WHERE commit_id = $1",
                )
                .bind(commit)
                .fetch_one(&pool)
                .await
                .unwrap();
            assert_eq!(stored.0, if status == Some(10) { 10 } else { 6 });
            if status == Some(10) {
                assert_eq!(stored.1, Some(format!("/nix/store/test-{commit}.drv")));
                assert_eq!(stored.2, Some(format!("/nix/store/test-{commit}")));
                assert_eq!(stored.3, stored.2);
            } else {
                assert_eq!((stored.1, stored.2, stored.3), (None, None, None));
            }
            assert_eq!(
                stored.4,
                json!({
                    "evaluation_attempt": number, "evaluation_failure": true,
                    "evaluation_failure_error": evaluation.confirmed_failures[0].error,
                })
            );
            let (flat, matrix) = fetch_matrix(&pool, commit).await.unwrap();
            // The compatibility handler maps eval_failed + error_message to
            // nix_eval_failure + the real diagnostic, independently of status10.
            assert_eq!(flat[0].eval_status, "eval_failed");
            assert_eq!(
                flat[0].error_message.as_deref(),
                Some("actual Nix module evaluation failure")
            );
            assert_eq!(flat[0].policy_requirements_met, Some(false));
            assert_eq!(matrix.rows[0].cells, vec![EvalMatrixCell::Unknown]);
            assert_eq!(matrix.rows[0].details, vec![None]);

            for invalid in [
                json!({"evaluation_attempt": number, "evaluation_failure": "true",
                    "evaluation_failure_error": "must not leak"}),
                json!({"evaluation_attempt": number, "evaluation_failure": true,
                    "evaluation_failure_error": 123}),
                json!({"evaluation_attempt": number - 1, "evaluation_failure": true,
                    "evaluation_failure_error": "must not leak"}),
            ] {
                sqlx::query("UPDATE derivations SET policy_results = $2 WHERE commit_id = $1")
                    .bind(commit)
                    .bind(invalid)
                    .execute(&pool)
                    .await
                    .unwrap();
                let (flat, matrix) = fetch_matrix(&pool, commit).await.unwrap();
                assert_ne!(flat[0].eval_status, "eval_failed");
                assert!(flat[0].error_message.is_none());
                assert!(flat[0].policy_requirements_met.is_none());
                assert_eq!(matrix.rows[0].cells, vec![EvalMatrixCell::Unknown]);
                assert_eq!(matrix.rows[0].details, vec![None]);
            }
            sqlx::query("UPDATE derivations SET policy_results = $2 WHERE commit_id = $1")
                .bind(commit)
                .bind(stored.4)
                .execute(&pool)
                .await
                .unwrap();

            // A queued retry is represented before the commit attempt counter
            // advances. It must already hide the previous native diagnostic.
            sqlx::query("INSERT INTO evaluation_attempts (commit_id, attempt_number, status) VALUES ($1, $2, 'queued')")
                .bind(commit).bind(number + 1).execute(&pool).await.unwrap();
            let (flat, _) = fetch_matrix(&pool, commit).await.unwrap();
            assert_ne!(flat[0].eval_status, "eval_failed");
            assert!(flat[0].error_message.is_none());
            assert!(flat[0].policy_requirements_met.is_none());
            assert_eq!(flat[0].policy_results, json!({}));
        }
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires authoritative server-regressions sandbox PostgreSQL"]
    async fn confirmed_native_finalizer_cancellation_writes_no_failure_evidence(pool: PgPool) {
        use crate::models::evaluate_with_policies::{
            EvaluationFinalizeOutcome, finalize_evaluation_attempt,
        };
        for existing in [false, true] {
            let (commit, _, number) = commit(&pool).await;
            if existing {
                sqlx::query("INSERT INTO derivations (commit_id, derivation_type, derivation_name, status_id, policy_results) VALUES ($1, 'nixos', 'shared', 10, '{\"evaluation_attempt\":0}')")
                    .bind(commit).execute(&pool).await.unwrap();
            }
            let before: Vec<Value> =
                sqlx::query_scalar("SELECT to_jsonb(d) FROM derivations d WHERE commit_id = $1")
                    .bind(commit)
                    .fetch_all(&pool)
                    .await
                    .unwrap();
            crate::queries::commits::cancel_commit_evaluation(&pool, commit)
                .await
                .unwrap();
            assert!(matches!(
                finalize_evaluation_attempt(&pool, commit, number, &confirmed_failure_plan())
                    .await
                    .unwrap(),
                EvaluationFinalizeOutcome::Cancelled
            ));
            let after: Vec<Value> =
                sqlx::query_scalar("SELECT to_jsonb(d) FROM derivations d WHERE commit_id = $1")
                    .bind(commit)
                    .fetch_all(&pool)
                    .await
                    .unwrap();
            assert_eq!(before, after);
            let snapshots: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM evaluation_snapshots WHERE commit_id = $1",
            )
            .bind(commit)
            .fetch_one(&pool)
            .await
            .unwrap();
            assert_eq!(snapshots, 0);
        }
    }

    #[test]
    fn native_failure_projection_rejects_stale_or_malformed_diagnostics() {
        let mut snapshot = plan();
        let policy = snapshot.columns[0].policy_version_id;
        for (marker, flag, error) in [
            (json!(1), json!(true), json!("old failure")),
            (json!("2"), json!(true), json!("old failure")),
            (json!(2.5), json!(true), json!("old failure")),
            (Value::Null, json!(true), json!("old failure")),
            (json!(2), json!("true"), json!("old failure")),
            (json!(2), json!(false), json!("old failure")),
            (json!(2), Value::Null, json!("old failure")),
            (json!(2), json!(true), json!(123)),
            (json!(2), json!(true), Value::Null),
            (json!(2), json!(true), json!("  ")),
        ] {
            for status in ["evaluated", "eval_failed"] {
                let mut row = outcome(policy, marker.clone(), json!({"passed": true}));
                row.eval_status = status.into();
                row.error_message = Some("old build message".into());
                row.policy_results["evaluation_failure"] = flag.clone();
                row.policy_results["evaluation_failure_error"] = error.clone();
                project_native_failure(&mut row, 2);
                assert_eq!(row.eval_status, "unavailable");
                assert!(row.error_message.is_none());
                assert!(row.policy_requirements_met.is_none());
                assert_eq!(row.policy_results, json!({}));
                project_outcomes(&mut snapshot, &[row]);
                assert_eq!(snapshot.rows[0].cells, vec![EvalMatrixCell::Unknown]);
                assert_eq!(snapshot.rows[0].details, vec![None]);
            }
        }
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires authoritative server-regressions sandbox PostgreSQL"]
    async fn isolated_mock_projection_matches_attempt_and_rejects_ambiguous_native(pool: PgPool) {
        let (commit, attempt_id, number) = commit(&pool).await;
        let mut captured = plan();
        captured.evaluation_attempt_id = Some(attempt_id);
        captured.attempt_number = number;
        let policy = captured.columns[0].policy_version_id;
        let mut tx = pool.begin().await.unwrap();
        insert_plan(&mut tx, commit, &captured).await.unwrap();
        tx.commit().await.unwrap();
        assert!(!fetch_matrix(&pool, commit).await.unwrap().1.mock_evidence);
        assert_eq!(
            fetch_matrix(&pool, commit).await.unwrap().1.rows[0].cells,
            vec![EvalMatrixCell::Unknown]
        );
        let mock = |marker| {
            json!({
                "execution_mode": "mock", "evaluation_attempt": marker,
                "global": {"cfAgentEnabled": {"passed": true, "strict": true,
                    "blocking": false, "details": "[MOCK] agent"}},
                "assigned": {policy.to_string(): {"passed": true, "blocking": false,
                    "details": "[MOCK] pass"}}
            })
        };
        assert!(
            sqlx::query(
                "INSERT INTO evaluation_mock_outcomes
                 (evaluation_attempt_id, commit_id, attempt_number, configuration_name,
                  policy_results, cf_agent_enabled, policy_requirements_met)
                 VALUES ($1, $2, $3, 'shared', $4, TRUE, TRUE)"
            )
            .bind(attempt_id)
            .bind(commit)
            .bind(number)
            .bind(mock(number + 1))
            .execute(&pool)
            .await
            .is_err()
        );
        sqlx::query(
            "INSERT INTO evaluation_mock_outcomes
                 (evaluation_attempt_id, commit_id, attempt_number, configuration_name,
                  policy_results, cf_agent_enabled, policy_requirements_met)
                 VALUES ($1, $2, $3, 'shared', $4, TRUE, TRUE)",
        )
        .bind(attempt_id)
        .bind(commit)
        .bind(number)
        .bind(mock(number))
        .execute(&pool)
        .await
        .unwrap();
        let (flat, matrix) = fetch_matrix(&pool, commit).await.unwrap();
        assert!(matrix.mock_evidence);
        assert_eq!(flat.len(), 1);
        assert_eq!(
            flat[0].policy_results["global"]["cfAgentEnabled"]["passed"],
            true
        );
        assert_eq!(matrix.rows[0].cells, vec![EvalMatrixCell::Pass]);
        assert_eq!(matrix.rows[0].details, vec![Some("[MOCK] pass".into())]);
        assert_eq!(matrix.rows[1].cells, vec![EvalMatrixCell::Unknown]);
        // A stale/manual native carrier cannot mask exact current mock evidence.
        sqlx::query(
            "INSERT INTO derivations (commit_id, derivation_type, derivation_name,
                     status_id, policy_results, policy_requirements_met)
                     VALUES ($1, 'nixos', 'shared', 5, $2, TRUE)",
        )
        .bind(commit)
        .bind(outcome(policy, json!(number + 1), json!({"passed": false})).policy_results)
        .execute(&pool)
        .await
        .unwrap();
        assert_eq!(
            fetch_matrix(&pool, commit).await.unwrap().1.rows[0].cells,
            vec![EvalMatrixCell::Pass]
        );
        sqlx::query("UPDATE derivations SET policy_results = '{}' WHERE commit_id = $1")
            .bind(commit)
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(
            fetch_matrix(&pool, commit).await.unwrap().1.rows[0].cells,
            vec![EvalMatrixCell::Pass]
        );
        sqlx::query("UPDATE derivations SET policy_results = $2 WHERE commit_id = $1")
            .bind(commit)
            .bind(outcome(policy, json!(number), json!({"passed": true})).policy_results)
            .execute(&pool)
            .await
            .unwrap();
        let (flat, ambiguous) = fetch_matrix(&pool, commit).await.unwrap();
        assert!(ambiguous.mock_evidence);
        assert_eq!(ambiguous.rows[0].cells, vec![EvalMatrixCell::Unknown]);
        assert_eq!(ambiguous.rows[0].details, vec![None]);
        assert!(flat[0].policy_requirements_met.is_none());
        assert!(flat[0].policy_results.get("global").is_none());
        let counts: (i64, i64, i64) = sqlx::query_as(
            "SELECT passed_count, policy_failed_count,
            eval_failed_count FROM view_evaluation_mock_counts WHERE commit_id = $1",
        )
        .bind(commit)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(counts, (0, 0, 0));
        let stored: Value =
            sqlx::query_scalar("SELECT payload FROM evaluation_policy_plans WHERE commit_id = $1")
                .bind(commit)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(stored, serde_json::to_value(&captured).unwrap());
        assert_eq!(stored["mock_evidence"], false);
        sqlx::query("UPDATE evaluation_attempts SET status = 'complete' WHERE id = $1")
            .bind(attempt_id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO evaluation_attempts (commit_id, attempt_number, status)
                     VALUES ($1, $2, 'queued')",
        )
        .bind(commit)
        .bind(number + 1)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("UPDATE commits SET evaluation_status = 'pending' WHERE id = $1")
            .bind(commit)
            .execute(&pool)
            .await
            .unwrap();
        let (flat, retry) = fetch_matrix(&pool, commit).await.unwrap();
        assert!(!retry.mock_evidence);
        assert_eq!(retry.attempt_number, number + 1);
        assert_eq!(retry.provenance, EvalMatrixProvenance::LegacyUnavailable);
        assert!(
            flat.iter()
                .all(|row| row.policy_results.get("execution_mode").is_none())
        );
        assert!(flat.iter().all(|row| row.policy_requirements_met.is_none()));
        // Current isolated evidence remains labelled even without a plan. This
        // direct fixture insert does not invoke evaluator or build machinery.
        sqlx::query(
            "INSERT INTO evaluation_mock_outcomes
                 (evaluation_attempt_id, commit_id, attempt_number, configuration_name,
                  policy_results, cf_agent_enabled, policy_requirements_met)
             SELECT id, commit_id, attempt_number, 'shared', $3, TRUE, TRUE
             FROM evaluation_attempts WHERE commit_id = $1 AND attempt_number = $2",
        )
        .bind(commit)
        .bind(number + 1)
        .bind(mock(number + 1))
        .execute(&pool)
        .await
        .unwrap();
        let legacy_mock = fetch_matrix(&pool, commit).await.unwrap().1;
        assert!(legacy_mock.mock_evidence);
        assert_eq!(
            legacy_mock.provenance,
            EvalMatrixProvenance::LegacyUnavailable
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires authoritative server-regressions sandbox PostgreSQL"]
    async fn capture_is_atomic_first_write_immutable_and_retains_partial_rows(pool: PgPool) {
        let (commit, id, number) = commit(&pool).await;
        let mut plan = plan();
        plan.evaluation_attempt_id = Some(id);
        plan.attempt_number = number;
        let mut tx = pool.begin().await.unwrap();
        assert_eq!(lock_attempt(&mut tx, commit, number).await.unwrap(), id);
        insert_plan(&mut tx, commit, &plan).await.unwrap();
        tx.rollback().await.unwrap();
        let mut projected = plan.clone();
        projected.mock_evidence = true;
        let mut tx = pool.begin().await.unwrap();
        assert!(insert_plan(&mut tx, commit, &projected).await.is_err());
        tx.rollback().await.unwrap();
        projected.mock_evidence = false;
        for field in ["commit_sha", "attempt_status", "started_at", "completed_at"] {
            let mut value = serde_json::to_value(&plan).unwrap();
            value[field] = if matches!(field, "started_at" | "completed_at") {
                json!("2026-10-16T00:00:00Z")
            } else {
                json!("projected context")
            };
            let context: EvalPolicyMatrixSnapshot = serde_json::from_value(value).unwrap();
            let mut tx = pool.begin().await.unwrap();
            assert!(insert_plan(&mut tx, commit, &context).await.is_err());
            tx.rollback().await.unwrap();
        }
        projected.rows[0]
            .details
            .push(Some("read-only evidence".into()));
        let mut tx = pool.begin().await.unwrap();
        assert!(insert_plan(&mut tx, commit, &projected).await.is_err());
        tx.rollback().await.unwrap();
        assert_eq!(
            fetch_matrix(&pool, commit).await.unwrap().1.provenance,
            EvalMatrixProvenance::LegacyUnavailable
        );
        let mut tx = pool.begin().await.unwrap();
        insert_plan(&mut tx, commit, &plan).await.unwrap();
        tx.commit().await.unwrap();
        let mut tx = pool.begin().await.unwrap();
        assert!(lock_attempt(&mut tx, commit, number).await.is_err());
        tx.rollback().await.unwrap();
        assert!(
            sqlx::query("UPDATE evaluation_policy_plans SET payload = '{}' WHERE commit_id = $1")
                .bind(commit)
                .execute(&pool)
                .await
                .is_err()
        );
        assert!(
            sqlx::query("DELETE FROM evaluation_policy_plans WHERE commit_id = $1")
                .bind(commit)
                .execute(&pool)
                .await
                .is_err()
        );
        let projected = fetch_matrix(&pool, commit).await.unwrap().1;
        assert_eq!(projected.rows.len(), 2);
        assert!(
            projected
                .rows
                .iter()
                .all(|r| r.cells == vec![EvalMatrixCell::Unknown])
        );
        assert_eq!(projected.rows[0].policies[0].memberships.len(), 2);
        assert_eq!(projected.columns[0].name, "Version-owned name");
        let policy = plan.columns[0].policy_version_id;
        let evidence = outcome(
            policy,
            json!(number + 1),
            json!({"passed": true, "details": "stale evidence"}),
        )
        .policy_results;
        sqlx::query("INSERT INTO derivations (commit_id, derivation_type, derivation_name, derivation_target,
                     status_id, attempt_count, cf_agent_enabled, policy_requirements_met, policy_results)
                     VALUES ($1, 'nixos', 'shared', 'shared', 5, 0, TRUE, TRUE, $2)")
            .bind(commit)
            .bind(evidence)
            .execute(&pool)
            .await
            .unwrap();
        let (flat, stale) = fetch_matrix(&pool, commit).await.unwrap();
        assert_eq!(stale.rows[0].cells, vec![EvalMatrixCell::Unknown]);
        assert_eq!(stale.rows[0].details, vec![None]);
        assert!(flat[0].policy_requirements_met.is_none());
        sqlx::query("UPDATE derivations SET policy_results = $2 WHERE commit_id = $1")
            .bind(commit)
            .bind(
                outcome(
                    policy,
                    json!(number),
                    json!({"passed": true, "details": "current evidence"}),
                )
                .policy_results,
            )
            .execute(&pool)
            .await
            .unwrap();
        let matched = fetch_matrix(&pool, commit).await.unwrap().1;
        assert!(!matched.mock_evidence);
        assert_eq!(matched.rows[0].cells, vec![EvalMatrixCell::Pass]);
        assert_eq!(
            matched.rows[0].details,
            vec![Some("current evidence".to_string())]
        );
        assert_eq!(matched.rows[1].cells, vec![EvalMatrixCell::Unknown]);
        assert_eq!(matched.rows[1].details, vec![None]);
        sqlx::query("UPDATE derivations SET status_id = 3 WHERE commit_id = $1")
            .bind(commit)
            .execute(&pool)
            .await
            .unwrap();
        let (flat, pending) = fetch_matrix(&pool, commit).await.unwrap();
        assert_eq!(pending.rows[0].cells, vec![EvalMatrixCell::Unknown]);
        assert_eq!(pending.rows[0].details, vec![None]);
        assert!(flat[0].policy_requirements_met.is_none());
        // Build failure does not erase valid matching-attempt policy evidence.
        sqlx::query("UPDATE derivations SET status_id = 12 WHERE commit_id = $1")
            .bind(commit)
            .execute(&pool)
            .await
            .unwrap();
        let built = fetch_matrix(&pool, commit).await.unwrap().1;
        assert_eq!(built.rows[0].cells, vec![EvalMatrixCell::Pass]);
        assert_eq!(built.rows[0].details, vec![Some("current evidence".into())]);
        let mut tx = pool.begin().await.unwrap();
        assert!(insert_plan(&mut tx, commit, &matched).await.is_err());
        tx.rollback().await.unwrap();
        // The fake catalog IDs deliberately do not exist. Reads must use only
        // the captured metadata and must never rewrite it with outcome cells.
        let stored: Value =
            sqlx::query_scalar("SELECT payload FROM evaluation_policy_plans WHERE commit_id = $1")
                .bind(commit)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(stored, serde_json::to_value(&plan).unwrap());
        assert!(
            sqlx::query("DELETE FROM commits WHERE id = $1")
                .bind(commit)
                .execute(&pool)
                .await
                .is_ok()
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires authoritative server-regressions sandbox PostgreSQL"]
    async fn capture_rejects_wrong_attempt_owner_and_retry_cannot_read_old_plan(pool: PgPool) {
        let (commit, id, number) = commit(&pool).await;
        let mut plan = plan();
        plan.evaluation_attempt_id = Some(id);
        plan.attempt_number = number + 1;
        let mut tx = pool.begin().await.unwrap();
        assert!(insert_plan(&mut tx, commit, &plan).await.is_err());
        tx.rollback().await.unwrap();
        plan.attempt_number = number;
        let mut tx = pool.begin().await.unwrap();
        insert_plan(&mut tx, commit, &plan).await.unwrap();
        tx.commit().await.unwrap();
        sqlx::query("UPDATE evaluation_attempts SET status = 'failed' WHERE id = $1")
            .bind(id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO evaluation_attempts (commit_id, attempt_number, status) VALUES ($1, $2, 'queued')")
            .bind(commit)
            .bind(number + 1)
            .execute(&pool)
            .await
            .unwrap();
        // Queuing a retry does not increment the started-attempt counter.
        sqlx::query("UPDATE commits SET evaluation_status = 'pending' WHERE id = $1")
            .bind(commit)
            .execute(&pool)
            .await
            .unwrap();
        let next = fetch_matrix(&pool, commit).await.unwrap().1;
        assert_eq!(next.attempt_number, number + 1);
        assert_eq!(next.provenance, EvalMatrixProvenance::LegacyUnavailable);
        assert!(next.bundles.is_empty());
    }
}
