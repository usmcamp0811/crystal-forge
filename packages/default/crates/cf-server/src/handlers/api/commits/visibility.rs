//! Authorizes indivisible evaluation evidence against current flake ownership.
//!
//! Logs and mutable native graphs can contain cross-configuration information.
//! They cannot use the matrix's per-host projection or text-based redaction.

use axum::http::{HeaderMap, StatusCode};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::api::eval_policy_matrix::{EvalMatrixProvenance, EvalPolicyMatrixSnapshot};
use crate::handlers::api::rbac;

/// Resolves an authenticated viewer's scope; `None` denotes an administrator.
///
/// # Errors
/// Returns 403 for missing, inactive, expired, or insufficient identity.
pub(super) async fn viewer_scope(
    pool: &PgPool,
    headers: &HeaderMap,
) -> Result<Option<Uuid>, StatusCode> {
    let (user, roles) = rbac::authenticated_user_roles(pool, headers)
        .await
        .ok_or(StatusCode::FORBIDDEN)?;
    if !rbac::has_viewer_or_above_role(&roles) {
        return Err(StatusCode::FORBIDDEN);
    }
    Ok((!rbac::has_admin_role(&roles)).then_some(user))
}

/// Opens a read-only repeatable-read snapshot authorized for whole-flake data.
///
/// The caller must first authenticate a viewer. Only administrators may supply
/// `None`. Missing and inaccessible commits both return `None`, before evidence
/// reads. Nonadministrators need at least one current registered host, membership
/// for EVERY host on the flake (including inactive and undiscovered hosts), and
/// an inventory for the exact latest attempt UUID, commit, and number. Every
/// discovered name must map to a current registered configuration. Shared names
/// never permit one visible host to authorize a hidden host. Blank configured
/// names use the hostname; discovered names remain exact and case-sensitive.
/// The exact selected attempt must also have a valid immutable captured plan.
/// Every captured row must retain its original system UUID and configuration on
/// this flake with current membership. Deleting, replacing, or reconfiguring a
/// captured host cannot transfer authority over historical unstructured data to
/// another host with the same configuration name. Missing or malformed plans
/// fail closed for nonadministrators; administrators do not require a plan.
///
/// The returned transaction MUST also read the evidence. A second pool read
/// would separate revocation/system moves from evidence under different MVCC
/// snapshots. Each stream send needs a new authorization snapshot; a successful
/// handshake does not grant a persistent lease. Authorization is snapshot-based,
/// so revocation affects the next read, not an already authorized response.
///
/// # Errors
/// Returns database errors without reading logs or derivation outcomes.
pub(super) async fn full_flake_snapshot(
    pool: &PgPool,
    commit_id: i32,
    user: Option<Uuid>,
) -> Result<Option<Transaction<'_, Postgres>>, sqlx::Error> {
    let mut tx = pool.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
        .execute(&mut *tx)
        .await?;
    let allowed: bool = sqlx::query_scalar(
        r#"
        SELECT EXISTS (
            SELECT 1 FROM commits c
            WHERE c.id = $1 AND (
                $2::uuid IS NULL OR (
                    EXISTS (SELECT 1 FROM systems s WHERE s.flake_id = c.flake_id)
                    AND NOT EXISTS (
                        SELECT 1 FROM systems s WHERE s.flake_id = c.flake_id
                        AND NOT EXISTS (
                            SELECT 1 FROM user_environment_memberships m
                            WHERE m.user_id = $2 AND m.environment_id = s.environment_id
                        )
                    )
                    AND EXISTS (
                        SELECT 1 FROM evaluation_attempts a
                        JOIN evaluation_attempt_inventory i
                          ON i.evaluation_attempt_id = a.id
                         AND i.commit_id = a.commit_id
                         AND i.attempt_number = a.attempt_number
                        WHERE a.commit_id = c.id
                          AND a.id = (
                              SELECT latest.id FROM evaluation_attempts latest
                              WHERE latest.commit_id = c.id
                              ORDER BY latest.attempt_number DESC LIMIT 1
                          )
                          AND NOT EXISTS (
                              SELECT 1 FROM unnest(i.configuration_names) AS discovered(name)
                              WHERE NOT EXISTS (
                                  SELECT 1 FROM systems s
                                  WHERE s.flake_id = c.flake_id
                                    AND COALESCE(NULLIF(BTRIM(s.system_configuration_name), ''), s.hostname)
                                        = discovered.name
                              )
                          )
                    )
                )
            )
        )
        "#,
    )
    .bind(commit_id)
    .bind(user)
    .fetch_one(&mut *tx)
    .await?;
    if !allowed {
        tx.rollback().await?;
        return Ok(None);
    }
    if let Some(user) = user {
        // SECURITY: Current registry coverage alone cannot authorize historical
        // diagnostics. Keep immutable identity checks in this same snapshot,
        // before any log history or mutable derivation outcomes are accessed.
        let selected = sqlx::query_as::<_, (Uuid, i32, serde_json::Value)>(
            "SELECT a.id, a.attempt_number, p.payload
             FROM evaluation_attempts a JOIN evaluation_policy_plans p
               ON p.evaluation_attempt_id = a.id AND p.commit_id = a.commit_id
              AND p.attempt_number = a.attempt_number
             WHERE a.commit_id = $1
               AND a.id = (SELECT latest.id FROM evaluation_attempts latest
                           WHERE latest.commit_id = $1
                           ORDER BY latest.attempt_number DESC LIMIT 1)",
        )
        .bind(commit_id)
        .fetch_optional(&mut *tx)
        .await?;
        let plan = selected.and_then(|(id, number, payload)| {
            let plan = serde_json::from_value::<EvalPolicyMatrixSnapshot>(payload).ok()?;
            (plan.provenance == EvalMatrixProvenance::Captured
                && plan.evaluation_attempt_id == Some(id)
                && plan.attempt_number == number)
                .then_some(plan)
        });
        let Some(plan) = plan else {
            tx.rollback().await?;
            return Ok(None);
        };
        let current: std::collections::HashMap<Uuid, String> = sqlx::query_as::<_, (Uuid, String)>(
            "SELECT s.id,
                    COALESCE(NULLIF(BTRIM(s.system_configuration_name), ''), s.hostname)
             FROM systems s JOIN commits c ON c.flake_id = s.flake_id
             WHERE c.id = $1 AND EXISTS (
                 SELECT 1 FROM user_environment_memberships m
                 WHERE m.user_id = $2 AND m.environment_id = s.environment_id)",
        )
        .bind(commit_id)
        .bind(user)
        .fetch_all(&mut *tx)
        .await?
        .into_iter()
        .collect();
        if !plan.rows.iter().all(|row| {
            row.system_id
                .is_some_and(|id| current.get(&id) == Some(&row.configuration_name))
        }) {
            tx.rollback().await?;
            return Ok(None);
        }
    }
    Ok(Some(tx))
}

/// Reads commit log history only after whole-flake authorization in one snapshot.
///
/// Logs remain commit-scoped, not pinned-attempt evidence. No string redaction
/// can make a mixed-scope log safe. `None` has the same meaning as in
/// [`full_flake_snapshot`].
///
/// # Errors
/// Returns SQL errors; denied callers never access the log table.
pub(super) async fn logs_for_user(
    pool: &PgPool,
    commit_id: i32,
    user: Option<Uuid>,
) -> Result<Option<Vec<crate::queries::eval_logs::EvalLogRow>>, sqlx::Error> {
    let Some(mut tx) = full_flake_snapshot(pool, commit_id, user).await? else {
        return Ok(None);
    };
    let rows =
        crate::queries::eval_logs::fetch_eval_logs_by_commit_in_tx(&mut tx, commit_id).await?;
    tx.commit().await?;
    Ok(Some(rows))
}

/// Reads the latest route's mutable native graph with whole-flake authority.
///
/// This is NOT pinned-attempt evidence: native closure/build state can change
/// after evaluation. The count semantics match `fetch_eval_dependency_breakdown`.
/// Nonadministrators also fail closed if a retained native configuration no
/// longer maps to the current registry, even if latest discovery omitted it.
///
/// # Errors
/// Returns SQL errors. Denied callers never read derivation outcomes.
pub(super) async fn graph_for_user(
    pool: &PgPool,
    commit_id: i32,
    user: Option<Uuid>,
) -> Result<Option<Vec<crate::queries::commits::EvalDependencyPackageRow>>, sqlx::Error> {
    let Some(mut tx) = full_flake_snapshot(pool, commit_id, user).await? else {
        return Ok(None);
    };
    if user.is_some() {
        let unmanaged: bool = sqlx::query_scalar(
            "SELECT EXISTS (
                SELECT 1 FROM derivations d JOIN commits c ON c.id = d.commit_id
                WHERE d.commit_id = $1 AND d.derivation_type = 'nixos'
                  AND NOT EXISTS (
                      SELECT 1 FROM systems s WHERE s.flake_id = c.flake_id
                        AND COALESCE(NULLIF(BTRIM(s.system_configuration_name), ''), s.hostname)
                            = d.derivation_name
                  )
            )",
        )
        .bind(commit_id)
        .fetch_one(&mut *tx)
        .await?;
        if unmanaged {
            tx.rollback().await?;
            return Ok(None);
        }
    }
    // COMPATIBILITY: Preserve the existing mutable graph query's count rules.
    // This local executor form keeps authorization and evidence in one snapshot.
    let rows = sqlx::query_as::<_, crate::queries::commits::EvalDependencyPackageRow>(
        r#"
        SELECT
            COALESCE(NULLIF(BTRIM(d.derivation_name), ''), 'unknown') AS package_name,
            (d.closure_total IS NOT NULL) AS closure_counted,
            CASE
                WHEN d.closure_total IS NOT NULL THEN COALESCE(d.closure_cached, 0)::BIGINT
                WHEN d.status_id = 10 OR (d.store_path IS NOT NULL AND d.store_path != '')
                    THEN 1::BIGINT
                ELSE 0::BIGINT
            END AS ready_count,
            CASE
                WHEN d.closure_total IS NOT NULL
                    THEN GREATEST(d.closure_total - COALESCE(d.closure_cached, 0), 0)::BIGINT
                WHEN d.status_id IN (5, 7, 8) AND (d.store_path IS NULL OR d.store_path = '')
                    THEN 1::BIGINT
                ELSE 0::BIGINT
            END AS pending_count,
            CASE WHEN d.status_id IN (6, 12)
                THEN COALESCE(NULLIF(d.closure_total, 0), 1)::BIGINT
                ELSE 0::BIGINT
            END AS failed_count
        FROM derivations d
        WHERE d.commit_id = $1 AND d.derivation_type = 'nixos'
        ORDER BY package_name ASC
        "#,
    )
    .bind(commit_id)
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(Some(rows))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::eval_policy_matrix::{EvalMatrixResolution, EvalMatrixRow};

    struct Fixture {
        user: Uuid,
        flake: i32,
        commit: i32,
        second_environment: Uuid,
        second_host: Uuid,
        plan: EvalPolicyMatrixSnapshot,
    }

    async fn fixture(pool: &PgPool, names: Option<&[&str]>) -> Fixture {
        let f = fixture_without_plan(pool, names).await;
        capture_plan(pool, &f).await;
        f
    }

    async fn capture_plan(pool: &PgPool, f: &Fixture) {
        let mut tx = pool.begin().await.unwrap();
        crate::queries::eval_policy_plans::insert_plan(&mut tx, f.commit, &f.plan)
            .await
            .unwrap();
        tx.commit().await.unwrap();
    }

    async fn fixture_without_plan(pool: &PgPool, names: Option<&[&str]>) -> Fixture {
        let suffix = Uuid::new_v4();
        let user: Uuid = sqlx::query_scalar(
            "INSERT INTO users(username, first_name, last_name, email)
             VALUES ($1, 'Scope', 'Reader', $2) RETURNING id",
        )
        .bind(format!("scope-{suffix}"))
        .bind(format!("{suffix}@example.invalid"))
        .fetch_one(pool)
        .await
        .unwrap();
        let mut environments = Vec::new();
        for label in ["visible", "foreign"] {
            let environment: Uuid =
                sqlx::query_scalar("INSERT INTO environments(name) VALUES ($1) RETURNING id")
                    .bind(format!("{label}-{suffix}"))
                    .fetch_one(pool)
                    .await
                    .unwrap();
            environments.push(environment);
        }
        sqlx::query(
            "INSERT INTO user_environment_memberships(user_id, environment_id) VALUES ($1,$2)",
        )
        .bind(user)
        .bind(environments[0])
        .execute(pool)
        .await
        .unwrap();
        let flake: i32 =
            sqlx::query_scalar("INSERT INTO flakes(name, repo_url) VALUES ($1, $2) RETURNING id")
                .bind(format!("scope-{suffix}"))
                .bind(format!("https://example.invalid/scope/{suffix}"))
                .fetch_one(pool)
                .await
                .unwrap();
        let commit: i32 = sqlx::query_scalar(
            "INSERT INTO commits(flake_id, git_commit_hash, commit_timestamp, evaluation_status)
             VALUES ($1, $2, NOW(), 'complete') RETURNING id",
        )
        .bind(flake)
        .bind(suffix.to_string())
        .fetch_one(pool)
        .await
        .unwrap();
        let mut hosts = Vec::new();
        for (index, config) in ["alpha", "beta"].into_iter().enumerate() {
            let host: Uuid = sqlx::query_scalar(
                "INSERT INTO systems(hostname, public_key, derivation, flake_id,
                     environment_id, system_configuration_name)
                 VALUES ($1, 'scope-key', '', $2, $3, $4) RETURNING id",
            )
            .bind(format!("{config}-{suffix}"))
            .bind(flake)
            .bind(environments[index])
            .bind(config)
            .fetch_one(pool)
            .await
            .unwrap();
            hosts.push(host);
        }
        if let Some(names) = names {
            let names: Vec<String> = names.iter().map(|name| (*name).into()).collect();
            sqlx::query(
                "INSERT INTO evaluation_attempt_inventory(evaluation_attempt_id, commit_id,
                     attempt_number, configuration_names, selected_configuration_names, execution_mode)
                 SELECT id, commit_id, attempt_number, $2, $2, 'native'
                 FROM evaluation_attempts WHERE commit_id = $1",
            )
            .bind(commit).bind(names).execute(pool).await.unwrap();
        }
        crate::queries::eval_logs::insert_eval_log(
            pool,
            commit,
            1,
            None,
            "alpha and beta private detail",
        )
        .await
        .unwrap();
        let (attempt_id, attempt_number): (Uuid, i32) = sqlx::query_as(
            "SELECT id, attempt_number FROM evaluation_attempts WHERE commit_id = $1
             ORDER BY attempt_number DESC LIMIT 1",
        )
        .bind(commit)
        .fetch_one(pool)
        .await
        .unwrap();
        let plan = EvalPolicyMatrixSnapshot {
            source_label: None,
            inventory: None,
            configuration_summary: None,
            configuration_failures: vec![],
            evaluation_attempt_id: Some(attempt_id),
            attempt_number,
            commit_sha: None,
            attempt_status: None,
            started_at: None,
            completed_at: None,
            mock_evidence: false,
            provenance: EvalMatrixProvenance::Captured,
            columns: vec![],
            bundles: vec![],
            empty_bundle_metadata_complete: true,
            rows: hosts
                .iter()
                .zip(["alpha", "beta"])
                .map(|(id, config)| EvalMatrixRow {
                    greenfield: None,
                    system_id: Some(*id),
                    system_name: format!("{config}-{suffix}"),
                    configuration_name: config.into(),
                    resolution: EvalMatrixResolution::Resolved,
                    evaluation_digest: None,
                    assigned_bundles: vec![],
                    policies: vec![],
                    cells: vec![],
                    details: vec![],
                })
                .collect(),
        };
        Fixture {
            user,
            flake,
            commit,
            second_environment: environments[1],
            second_host: hosts[1],
            plan,
        }
    }

    async fn grant_all(pool: &PgPool, f: &Fixture) {
        sqlx::query(
            "INSERT INTO user_environment_memberships(user_id, environment_id) VALUES ($1,$2)",
        )
        .bind(f.user)
        .bind(f.second_environment)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn allowed(pool: &PgPool, f: &Fixture) -> bool {
        if let Some(tx) = full_flake_snapshot(pool, f.commit, Some(f.user))
            .await
            .unwrap()
        {
            tx.commit().await.unwrap();
            true
        } else {
            false
        }
    }

    async fn denied_evidence(pool: &PgPool, f: &Fixture) {
        assert!(!allowed(pool, f).await);
        assert!(
            logs_for_user(pool, f.commit, Some(f.user))
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            graph_for_user(pool, f.commit, Some(f.user))
                .await
                .unwrap()
                .is_none()
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires isolated migration-backed PostgreSQL"]
    async fn mixed_environment_flake_denies_unstructured_evidence(pool: PgPool) {
        let f = fixture(&pool, Some(&["alpha", "beta"])).await;
        denied_evidence(&pool, &f).await;
        // Even a planned but undiscovered hidden host blocks unstructured data.
        let undiscovered = fixture(&pool, Some(&["alpha"])).await;
        denied_evidence(&pool, &undiscovered).await;
        assert_eq!(
            logs_for_user(&pool, f.commit, None)
                .await
                .unwrap()
                .unwrap()
                .len(),
            1
        );
        assert!(
            full_flake_snapshot(&pool, -1, None)
                .await
                .unwrap()
                .is_none()
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires isolated migration-backed PostgreSQL"]
    async fn revocation_rechecks_each_snapshot_and_keeps_reads_atomic(pool: PgPool) {
        let f = fixture(&pool, Some(&["alpha", "beta"])).await;
        grant_all(&pool, &f).await;
        let mut authorized = full_flake_snapshot(&pool, f.commit, Some(f.user))
            .await
            .unwrap()
            .unwrap();
        sqlx::query(
            "DELETE FROM user_environment_memberships WHERE user_id = $1 AND environment_id = $2",
        )
        .bind(f.user)
        .bind(f.second_environment)
        .execute(&pool)
        .await
        .unwrap();
        // An already authorized response has snapshot semantics. Every new
        // stream event/heartbeat opens a fresh snapshot and sees revocation.
        let rows =
            crate::queries::eval_logs::fetch_eval_logs_by_commit_in_tx(&mut authorized, f.commit)
                .await
                .unwrap();
        assert_eq!(rows.len(), 1);
        authorized.commit().await.unwrap();
        denied_evidence(&pool, &f).await;
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires isolated migration-backed PostgreSQL"]
    async fn unmanaged_discovery_name_denies_even_with_all_memberships(pool: PgPool) {
        for name in ["unregistered", " alpha ", "ALPHA", ""] {
            let f = fixture(&pool, Some(&["alpha", "beta", name])).await;
            grant_all(&pool, &f).await;
            denied_evidence(&pool, &f).await;
        }
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires isolated migration-backed PostgreSQL"]
    async fn missing_inventory_and_queued_retry_never_inherit_authority(pool: PgPool) {
        let missing = fixture(&pool, None).await;
        grant_all(&pool, &missing).await;
        denied_evidence(&pool, &missing).await;
        let f = fixture(&pool, Some(&["alpha", "beta"])).await;
        grant_all(&pool, &f).await;
        assert!(allowed(&pool, &f).await);
        sqlx::query("UPDATE evaluation_attempts SET status = 'failed' WHERE commit_id = $1")
            .bind(f.commit)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO evaluation_attempts(commit_id, attempt_number, status)
                     SELECT $1, MAX(attempt_number) + 1, 'queued' FROM evaluation_attempts WHERE commit_id = $1")
            .bind(f.commit).execute(&pool).await.unwrap();
        denied_evidence(&pool, &f).await;
        // Latest discovery cannot inherit the older attempt's captured plan.
        sqlx::query(
            "INSERT INTO evaluation_attempt_inventory(evaluation_attempt_id, commit_id,
                 attempt_number, configuration_names, selected_configuration_names, execution_mode)
             SELECT id, commit_id, attempt_number, ARRAY['alpha', 'beta'], ARRAY['alpha', 'beta'], 'native'
             FROM evaluation_attempts WHERE commit_id = $1 ORDER BY attempt_number DESC LIMIT 1",
        )
        .bind(f.commit).execute(&pool).await.unwrap();
        denied_evidence(&pool, &f).await;

        let missing_plan = fixture_without_plan(&pool, Some(&["alpha", "beta"])).await;
        grant_all(&pool, &missing_plan).await;
        denied_evidence(&pool, &missing_plan).await;
        assert!(
            logs_for_user(&pool, missing_plan.commit, None)
                .await
                .unwrap()
                .is_some()
        );
        // Never edit immutable captures to create adversarial payloads. Insert a
        // different first-write capture in each isolated fixture instead.
        for malformed in [
            "missing-fields",
            "invalid-rows",
            "invalid-uuid",
            "missing-uuid",
            "wrong-attempt",
            "wrong-number",
            "legacy",
        ] {
            let f = fixture_without_plan(&pool, Some(&["alpha", "beta"])).await;
            grant_all(&pool, &f).await;
            let mut payload = serde_json::to_value(&f.plan).unwrap();
            match malformed {
                "missing-fields" => payload = serde_json::json!({}),
                "invalid-rows" => payload["rows"] = serde_json::json!({}),
                "invalid-uuid" => payload["rows"][0]["system_id"] = serde_json::json!("invalid"),
                "missing-uuid" => payload["rows"][0]["system_id"] = serde_json::Value::Null,
                "wrong-attempt" => {
                    payload["evaluation_attempt_id"] = serde_json::json!(Uuid::new_v4())
                }
                "wrong-number" => {
                    payload["attempt_number"] = serde_json::json!(f.plan.attempt_number + 1)
                }
                "legacy" => payload["provenance"] = serde_json::json!("legacy_unavailable"),
                _ => unreachable!(),
            }
            sqlx::query("INSERT INTO evaluation_policy_plans(commit_id, attempt_number, evaluation_attempt_id, payload)
                         VALUES ($1, $2, $3, $4)")
                .bind(f.commit).bind(f.plan.attempt_number).bind(f.plan.evaluation_attempt_id)
                .bind(payload).execute(&pool).await.unwrap();
            denied_evidence(&pool, &f).await;
            assert!(
                logs_for_user(&pool, f.commit, None)
                    .await
                    .unwrap()
                    .is_some()
            );
        }
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires isolated migration-backed PostgreSQL"]
    async fn all_visible_configs_allow_logs_and_mutable_latest_graph(pool: PgPool) {
        let f = fixture(&pool, Some(&["alpha", "beta"])).await;
        grant_all(&pool, &f).await;
        assert!(allowed(&pool, &f).await);
        let logs = logs_for_user(&pool, f.commit, Some(f.user))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(logs[0].log_message, "alpha and beta private detail");
        assert!(
            graph_for_user(&pool, f.commit, Some(f.user))
                .await
                .unwrap()
                .unwrap()
                .is_empty()
        );
        // Normalize the registry only. Whitespace configuration names fall back
        // to the hostname; discovered names must still match that exact value.
        sqlx::query(
            "UPDATE systems SET hostname = 'beta', system_configuration_name = '  ' WHERE id = $1",
        )
        .bind(f.second_host)
        .execute(&pool)
        .await
        .unwrap();
        assert!(allowed(&pool, &f).await);
        sqlx::query("UPDATE systems SET system_configuration_name = ' beta ' WHERE id = $1")
            .bind(f.second_host)
            .execute(&pool)
            .await
            .unwrap();
        assert!(allowed(&pool, &f).await);
        // A deleted/moved configuration cannot retain inventory authority.
        sqlx::query("UPDATE systems SET flake_id = NULL WHERE id = $1")
            .bind(f.second_host)
            .execute(&pool)
            .await
            .unwrap();
        denied_evidence(&pool, &f).await;
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires isolated migration-backed PostgreSQL"]
    async fn shared_configuration_hidden_host_denies_even_when_inactive(pool: PgPool) {
        let mut f = fixture_without_plan(&pool, Some(&["alpha"])).await;
        sqlx::query("UPDATE systems SET system_configuration_name = 'alpha', is_active = false WHERE id = $1")
            .bind(f.second_host).execute(&pool).await.unwrap();
        f.plan.rows[1].configuration_name = "alpha".into();
        capture_plan(&pool, &f).await;
        denied_evidence(&pool, &f).await;

        for change in ["deleted", "replaced", "reconfigured", "moved"] {
            let mut f = fixture_without_plan(&pool, Some(&["alpha"])).await;
            sqlx::query("UPDATE systems SET system_configuration_name = 'alpha' WHERE id = $1")
                .bind(f.second_host)
                .execute(&pool)
                .await
                .unwrap();
            f.plan.rows[1].configuration_name = "alpha".into();
            capture_plan(&pool, &f).await;
            // The hidden host was captured even though evaluation deduplicated
            // its shared configuration. Current name coverage cannot erase it.
            denied_evidence(&pool, &f).await;
            if change == "deleted" || change == "replaced" {
                sqlx::query("DELETE FROM systems WHERE id = $1")
                    .bind(f.second_host)
                    .execute(&pool)
                    .await
                    .unwrap();
            } else if change == "reconfigured" {
                sqlx::query("UPDATE systems SET system_configuration_name = 'beta' WHERE id = $1")
                    .bind(f.second_host)
                    .execute(&pool)
                    .await
                    .unwrap();
            } else {
                sqlx::query("UPDATE systems SET flake_id = NULL WHERE id = $1")
                    .bind(f.second_host)
                    .execute(&pool)
                    .await
                    .unwrap();
            }
            if change == "replaced" {
                let replacement: Uuid = sqlx::query_scalar(
                    "INSERT INTO systems(hostname, public_key, derivation, flake_id, environment_id, system_configuration_name)
                     VALUES ($1, 'replacement-key', '', $2, $3, 'alpha') RETURNING id",
                )
                .bind(&f.plan.rows[1].system_name).bind(f.flake).bind(f.second_environment)
                .fetch_one(&pool).await.unwrap();
                assert_ne!(replacement, f.second_host);
            }
            grant_all(&pool, &f).await;
            // All remaining/current hosts are visible and discovery's alpha is
            // still registered. Only immutable row identity prevents disclosure.
            let all_current_visible: bool = sqlx::query_scalar(
                "SELECT NOT EXISTS (
                     SELECT 1 FROM systems s WHERE s.flake_id = $1 AND NOT EXISTS (
                         SELECT 1 FROM user_environment_memberships m
                         WHERE m.user_id = $2 AND m.environment_id = s.environment_id))",
            )
            .bind(f.flake)
            .bind(f.user)
            .fetch_one(&pool)
            .await
            .unwrap();
            assert!(all_current_visible);
            denied_evidence(&pool, &f).await;
            assert!(
                logs_for_user(&pool, f.commit, None)
                    .await
                    .unwrap()
                    .is_some()
            );
        }
        grant_all(&pool, &f).await;
        assert!(allowed(&pool, &f).await);
        sqlx::query("UPDATE systems SET environment_id = NULL WHERE id = $1")
            .bind(f.second_host)
            .execute(&pool)
            .await
            .unwrap();
        denied_evidence(&pool, &f).await;
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires isolated migration-backed PostgreSQL"]
    async fn missing_identity_or_registration_never_grants_vacuous_access(pool: PgPool) {
        let f = fixture(&pool, Some(&[])).await;
        assert!(
            full_flake_snapshot(&pool, f.commit, Some(Uuid::new_v4()))
                .await
                .unwrap()
                .is_none()
        );
        sqlx::query("UPDATE systems SET flake_id = NULL WHERE flake_id = $1")
            .bind(f.flake)
            .execute(&pool)
            .await
            .unwrap();
        denied_evidence(&pool, &f).await;
        assert!(
            logs_for_user(&pool, -1, Some(f.user))
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            graph_for_user(&pool, -1, Some(f.user))
                .await
                .unwrap()
                .is_none()
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires isolated migration-backed PostgreSQL"]
    async fn mutable_graph_preserves_counts_but_denies_unmanaged_retained_rows(pool: PgPool) {
        let f = fixture(&pool, Some(&["alpha", "beta"])).await;
        grant_all(&pool, &f).await;
        sqlx::query(
            "INSERT INTO derivations(commit_id, derivation_type, derivation_name,
                 derivation_target, status_id, attempt_count, closure_total, closure_cached)
             VALUES ($1, 'nixos', 'alpha', 'alpha', 5, 0, 12, 7)",
        )
        .bind(f.commit)
        .execute(&pool)
        .await
        .unwrap();
        let rows = graph_for_user(&pool, f.commit, Some(f.user))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].package_name, "alpha");
        assert!(rows[0].closure_counted);
        assert_eq!(
            (
                rows[0].ready_count,
                rows[0].pending_count,
                rows[0].failed_count
            ),
            (7, 5, 0)
        );
        sqlx::query(
            "INSERT INTO derivations(commit_id, derivation_type, derivation_name,
                 derivation_target, status_id, attempt_count)
             VALUES ($1, 'nixos', 'old-unmanaged', 'old-unmanaged', 6, 0)",
        )
        .bind(f.commit)
        .execute(&pool)
        .await
        .unwrap();
        assert!(
            graph_for_user(&pool, f.commit, Some(f.user))
                .await
                .unwrap()
                .is_none()
        );
        assert_eq!(
            graph_for_user(&pool, f.commit, None)
                .await
                .unwrap()
                .unwrap()
                .len(),
            2
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires isolated migration-backed PostgreSQL"]
    async fn stream_revalidation_observes_membership_roles_and_session_revocation(pool: PgPool) {
        use crate::auth::session::{SESSION_COOKIE_NAME, hash_token};
        use crate::handlers::agent_request::CFState;
        use crate::models::auth_identity::AuthRole;
        use crate::queries::auth_identity::{create_user_session, sync_user_role};

        let f = fixture(&pool, Some(&["alpha", "beta"])).await;
        grant_all(&pool, &f).await;
        sync_user_role(&pool, f.user, AuthRole::Viewer)
            .await
            .unwrap();
        let token = Uuid::new_v4().to_string();
        let token_hash = hash_token(&token);
        create_user_session(
            &pool,
            f.user,
            token_hash.clone(),
            chrono::Utc::now() + chrono::Duration::hours(1),
            None,
            None,
            "local".into(),
        )
        .await
        .unwrap();
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::COOKIE,
            format!("{SESSION_COOKIE_NAME}={token}").parse().unwrap(),
        );
        let state = CFState::new(
            pool.clone(),
            crate::config::ServerConfig::default(),
            std::sync::Arc::new(crate::queue::QueueNotifier::new()),
            crate::server::jobs::BackgroundJobRegistry::new(),
        );
        assert_eq!(
            super::super::authorize_eval_stream(&state, &headers, f.commit).await,
            Ok(())
        );
        sqlx::query(
            "DELETE FROM user_environment_memberships WHERE user_id = $1 AND environment_id = $2",
        )
        .bind(f.user)
        .bind(f.second_environment)
        .execute(&pool)
        .await
        .unwrap();
        assert_eq!(
            super::super::authorize_eval_stream(&state, &headers, f.commit).await,
            Err(StatusCode::NOT_FOUND)
        );
        // Admin authority must also be re-resolved, never cached at handshake.
        sync_user_role(&pool, f.user, AuthRole::Admin)
            .await
            .unwrap();
        assert_eq!(
            super::super::authorize_eval_stream(&state, &headers, f.commit).await,
            Ok(())
        );
        sync_user_role(&pool, f.user, AuthRole::Viewer)
            .await
            .unwrap();
        assert_eq!(
            super::super::authorize_eval_stream(&state, &headers, f.commit).await,
            Err(StatusCode::NOT_FOUND)
        );
        grant_all(&pool, &f).await;
        assert_eq!(
            super::super::authorize_eval_stream(&state, &headers, f.commit).await,
            Ok(())
        );
        crate::queries::auth_identity::invalidate_session_by_token_hash(&pool, &token_hash)
            .await
            .unwrap();
        assert_eq!(
            super::super::authorize_eval_stream(&state, &headers, f.commit).await,
            Err(StatusCode::FORBIDDEN)
        );

        use futures::{SinkExt, StreamExt};
        use tokio::time::{Duration, timeout};
        use tokio_tungstenite::tungstenite::{
            Error as WebSocketError, Message, client::IntoClientRequest, error::ProtocolError,
        };

        // Own only this ephemeral test listener. Unwinding any assertion aborts
        // its server task; neither fixture failure nor timeout can leave a
        // listener running on the task preview or a default application port.
        struct TestServer(tokio::task::JoinHandle<()>);
        impl Drop for TestServer {
            fn drop(&mut self) {
                self.0.abort();
            }
        }

        // The helper checks above deliberately invalidated the first session.
        // Use a fresh viewer session for the actual HTTP upgrade and stream.
        let network_token = Uuid::new_v4().to_string();
        create_user_session(
            &pool,
            f.user,
            hash_token(&network_token),
            chrono::Utc::now() + chrono::Duration::hours(1),
            None,
            None,
            "local".into(),
        )
        .await
        .unwrap();
        let listener = timeout(
            Duration::from_secs(5),
            tokio::net::TcpListener::bind("127.0.0.1:0"),
        )
        .await
        .expect("test listener bind timed out")
        .expect("test listener should bind");
        let address = listener.local_addr().unwrap();
        let app = axum::Router::new()
            .route(
                "/api/v1/commits/:commit_id/eval/stream",
                axum::routing::get(super::super::stream_eval_logs),
            )
            .with_state(state.clone());
        let mut server = TestServer(tokio::spawn(async move {
            axum::serve(listener, app)
                .await
                .expect("isolated test server should serve");
        }));
        let mut request = format!("ws://{address}/api/v1/commits/{}/eval/stream", f.commit)
            .into_client_request()
            .unwrap();
        request.headers_mut().insert(
            "Cookie",
            format!("{SESSION_COOKIE_NAME}={network_token}")
                .parse()
                .unwrap(),
        );
        let (mut client, response) = timeout(
            Duration::from_secs(5),
            tokio_tungstenite::connect_async(request),
        )
        .await
        .expect("WebSocket upgrade timed out")
        .expect("authorized viewer should upgrade");
        assert_eq!(response.status().as_u16(), 101);

        // Upgrade completion alone does not prove the broadcast subscription
        // exists. The handler's immediate first heartbeat follows subscription,
        // so use that frame as a protocol barrier instead of sleeping or racing
        // the replay/subscription boundary. This fresh state has no replay data.
        timeout(Duration::from_secs(5), async {
            loop {
                match client.next().await {
                    Some(Ok(Message::Ping(payload))) => {
                        client.send(Message::Pong(payload)).await.unwrap();
                        break;
                    }
                    Some(Ok(Message::Pong(_))) => continue,
                    other => panic!("expected initial subscribed heartbeat, received {other:?}"),
                }
            }
        })
        .await
        .expect("initial subscribed heartbeat timed out");

        let authorized_message = format!("authorized-network-{}", Uuid::new_v4());
        timeout(Duration::from_secs(5), async {
            crate::queries::eval_logs::insert_eval_log(
                &pool,
                f.commit,
                2,
                None,
                &authorized_message,
            )
            .await
            .unwrap();
            super::super::broadcast_eval_log(&state, f.commit, authorized_message.clone()).await;
            loop {
                match client.next().await {
                    Some(Ok(Message::Text(text))) => {
                        let event: super::super::EvalLogMessage =
                            serde_json::from_str(&text).expect("stream should send a log event");
                        assert!(
                            matches!(event, super::super::EvalLogMessage::Log { message }
                            if message == authorized_message)
                        );
                        break;
                    }
                    Some(Ok(Message::Ping(payload))) => {
                        client.send(Message::Pong(payload)).await.unwrap();
                    }
                    Some(Ok(Message::Pong(_))) => continue,
                    other => panic!("authorized log event was not delivered: {other:?}"),
                }
            }
        })
        .await
        .expect("authorized log roundtrip timed out");

        // Commit revocation before broadcasting. The next outgoing event must
        // trigger a fresh authorization check, not reuse handshake authority or
        // wait for the 20-second heartbeat. Bound the whole receive loop so
        // control frames cannot extend the five-second revocation assertion.
        timeout(Duration::from_secs(5), async {
            sqlx::query(
                "DELETE FROM user_environment_memberships WHERE user_id = $1 AND environment_id = $2",
            )
            .bind(f.user)
            .bind(f.second_environment)
            .execute(&pool)
            .await
            .unwrap();
        })
        .await
        .expect("membership revocation timed out");
        let protected_message = format!("protected-after-revocation-{}", Uuid::new_v4());
        timeout(Duration::from_secs(5), async {
            super::super::broadcast_eval_log(&state, f.commit, protected_message.clone()).await;
            loop {
                match client.next().await {
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(Message::Ping(_))) | Some(Ok(Message::Pong(_))) => continue,
                    Some(Ok(Message::Text(text))) => {
                        assert!(
                            !text.contains(&protected_message),
                            "revoked stream disclosed protected text"
                        );
                        panic!("revoked stream sent an unexpected text event");
                    }
                    // The handler fails closed by dropping the socket rather
                    // than sending a close frame. Tungstenite can report that
                    // transport EOF/reset as an error; other errors still fail.
                    Some(Err(WebSocketError::ConnectionClosed))
                    | Some(Err(WebSocketError::Protocol(
                        ProtocolError::ResetWithoutClosingHandshake,
                    ))) => break,
                    Some(Err(WebSocketError::Io(error)))
                        if matches!(
                            error.kind(),
                            std::io::ErrorKind::UnexpectedEof | std::io::ErrorKind::ConnectionReset
                        ) =>
                    {
                        break;
                    }
                    other => panic!("expected stream closure after revocation: {other:?}"),
                }
            }
        })
        .await
        .expect("revoked stream must close within five seconds of the event");

        server.0.abort();
        let _ = timeout(Duration::from_secs(5), &mut server.0)
            .await
            .expect("isolated test server should stop after abort");
    }
}
