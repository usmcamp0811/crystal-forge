//! Scoped summary regressions for disposable migration-backed databases.

use super::{list_eval_history_for_user, list_eval_queue_for_user};
use crate::api::models::{EvalHistoryParams, EvalQueueParams};
use sqlx::PgPool;
use uuid::Uuid;

/// Inserts fixture discovery for the latest attempt, which must have no inventory.
pub(super) async fn inventory(pool: &PgPool, commit: i32, names: &[&str], selected: &[&str]) {
    sqlx::query("INSERT INTO evaluation_attempt_inventory(evaluation_attempt_id,commit_id,attempt_number,configuration_names,selected_configuration_names,execution_mode) SELECT id,commit_id,attempt_number,$2,$3,'native' FROM evaluation_attempts WHERE commit_id=$1 ORDER BY attempt_number DESC LIMIT 1")
        .bind(commit).bind(names).bind(selected).execute(pool).await.unwrap();
}

async fn native(pool: &PgPool, commit: i32, name: &str, outcome: &str) {
    let (status, enabled, met) = match outcome {
        "pass" => (5, Some(true), Some(true)),
        "policy" => (5, Some(true), Some(false)),
        "failure" => (6, None, None),
        "pending" => (3, None, None),
        _ => (5, None, None),
    };
    sqlx::query("INSERT INTO derivations(commit_id,derivation_name,derivation_type,status_id,policy_results,cf_agent_enabled,policy_requirements_met) VALUES($1,$2,'nixos',$3,'{\"evaluation_attempt\":7}',$4,$5)")
        .bind(commit).bind(name).bind(status).bind(enabled).bind(met).execute(pool).await.unwrap();
}

async fn assert_summary(
    pool: &PgPool,
    commit: i32,
    user: Option<Uuid>,
    names: &[&str],
    expected: (i64, i64, i64, i64, i64, bool, bool),
) {
    let queue = list_eval_queue_for_user(pool, &EvalQueueParams::default(), user)
        .await
        .unwrap();
    let q = queue.rows.iter().find(|r| r.commit_id == commit).unwrap();
    assert_eq!(q.systems, names);
    assert_eq!(q.system_count, names.len() as i64);
    if q.inventory_available {
        assert_eq!(
            q.passed_count
                + q.policy_failed_count
                + q.eval_failed_count
                + q.policy_pending_count
                + q.policy_unavailable_count
                + q.unselected_count,
            q.system_count
        );
    }
    assert_eq!(
        (
            q.passed_count,
            q.policy_failed_count,
            q.eval_failed_count,
            q.policy_pending_count,
            q.policy_unavailable_count,
            q.inventory_available,
            q.policy_counts_available
        ),
        expected
    );
    let history = list_eval_history_for_user(pool, &EvalHistoryParams::default(), user)
        .await
        .unwrap();
    if matches!(
        q.evaluation_status.as_str(),
        "pending" | "in_progress" | "cancelling"
    ) {
        assert!(!history.items.iter().any(|row| row.commit_id == commit));
        return;
    }
    let h = history
        .items
        .iter()
        .find(|r| r.commit_id == commit)
        .unwrap();
    assert_eq!(h.system_count, q.system_count);
    assert_eq!(h.derivation_count, q.derivation_count);
    assert_eq!(h.unselected_count, q.unselected_count);
    assert_eq!(
        (
            h.passed_count,
            h.policy_failed_count,
            h.eval_failed_count,
            h.policy_pending_count,
            h.policy_unavailable_count,
            h.inventory_available,
            h.policy_counts_available
        ),
        expected
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires isolated migration-backed PostgreSQL"]
async fn exact_inventory_replaces_cached_41_and_excludes_unselected_outcomes(pool: PgPool) {
    let commit = super::tests::derivation_count_commit(&pool).await;
    sqlx::query("INSERT INTO commit_artifacts_cache(commit_id,nixos_configurations) SELECT $1, array_agg('cached-'||n) FROM generate_series(1,41) n")
        .bind(commit).execute(&pool).await.unwrap();
    inventory(&pool, commit, &["selected", "unselected"], &["selected"]).await;
    native(&pool, commit, "selected", "pass").await;
    native(&pool, commit, "unselected", "pass").await;
    native(&pool, commit, "outside-inventory", "failure").await;
    sqlx::query("INSERT INTO evaluation_mock_outcomes(evaluation_attempt_id,commit_id,attempt_number,configuration_name,policy_results,cf_agent_enabled,policy_requirements_met) SELECT id,commit_id,attempt_number,'outside-inventory','{\"evaluation_attempt\":7,\"execution_mode\":\"mock\"}',TRUE,TRUE FROM evaluation_attempts WHERE commit_id=$1")
        .bind(commit).execute(&pool).await.unwrap();
    assert_summary(
        &pool,
        commit,
        None,
        &["selected", "unselected"],
        (1, 0, 0, 0, 0, true, true),
    )
    .await;
    let all = list_eval_queue_for_user(&pool, &EvalQueueParams::default(), None)
        .await
        .unwrap();
    assert_eq!(
        all.rows
            .iter()
            .find(|r| r.commit_id == commit)
            .unwrap()
            .derivation_count,
        Some(2)
    );
    let q = list_eval_queue_for_user(
        &pool,
        &EvalQueueParams {
            search: Some("cached-41".into()),
            ..Default::default()
        },
        None,
    )
    .await
    .unwrap();
    assert!(!q.rows.iter().any(|r| r.commit_id == commit));
    let h = list_eval_history_for_user(
        &pool,
        &EvalHistoryParams {
            search: Some("cached-41".into()),
            ..Default::default()
        },
        None,
    )
    .await
    .unwrap();
    assert!(!h.items.iter().any(|r| r.commit_id == commit));
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires isolated migration-backed PostgreSQL"]
async fn inventory_partial_unknown_ambiguous_and_queued_retry_remain_pending(pool: PgPool) {
    let commit = super::tests::derivation_count_commit(&pool).await;
    sqlx::query("UPDATE commits SET evaluation_status='in_progress',evaluation_completed_at=NULL WHERE id=$1")
        .bind(commit).execute(&pool).await.unwrap();
    sqlx::query("UPDATE evaluation_attempts SET status='in_progress',started_at=NOW(),completed_at=NULL WHERE commit_id=$1")
        .bind(commit).execute(&pool).await.unwrap();
    native(&pool, commit, "pass", "pass").await;
    assert_summary(&pool, commit, None, &[], (0, 0, 0, 0, 0, false, false)).await;
    let names = [
        "ambiguous",
        "executing",
        "failure",
        "missing",
        "pass",
        "policy",
        "unknown",
    ];
    inventory(&pool, commit, &names, &names).await;
    native(&pool, commit, "policy", "policy").await;
    native(&pool, commit, "failure", "failure").await;
    native(&pool, commit, "unknown", "unknown").await;
    native(&pool, commit, "executing", "pending").await;
    assert_summary(&pool, commit, None, &names, (1, 1, 1, 3, 1, true, true)).await;
    native(&pool, commit, "ambiguous", "pass").await;
    assert_summary(&pool, commit, None, &names, (2, 1, 1, 2, 1, true, true)).await;
    sqlx::query("INSERT INTO evaluation_mock_outcomes(evaluation_attempt_id,commit_id,attempt_number,configuration_name,policy_results,cf_agent_enabled,policy_requirements_met) SELECT id,commit_id,attempt_number,'ambiguous','{\"evaluation_attempt\":7,\"execution_mode\":\"mock\"}',TRUE,TRUE FROM evaluation_attempts WHERE commit_id=$1")
        .bind(commit).execute(&pool).await.unwrap();
    assert_summary(&pool, commit, None, &names, (1, 1, 1, 2, 2, true, false)).await;
    sqlx::query(
        "UPDATE evaluation_attempts SET status='complete' WHERE commit_id=$1 AND attempt_number=7",
    )
    .bind(commit)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO evaluation_attempts(commit_id,attempt_number,status) VALUES($1,8,'queued')",
    )
    .bind(commit)
    .execute(&pool)
    .await
    .unwrap();
    assert_summary(&pool, commit, None, &[], (0, 0, 0, 0, 0, false, false)).await;
    sqlx::query("UPDATE evaluation_attempts SET status='in_progress',started_at=NOW() WHERE commit_id=$1 AND attempt_number=8")
        .bind(commit).execute(&pool).await.unwrap();
    sqlx::query("UPDATE commits SET evaluation_attempt_count=8 WHERE id=$1")
        .bind(commit)
        .execute(&pool)
        .await
        .unwrap();
    inventory(&pool, commit, &["retry-only"], &["retry-only"]).await;
    assert_summary(
        &pool,
        commit,
        None,
        &["retry-only"],
        (0, 0, 0, 1, 0, true, true),
    )
    .await;
}

async fn host(pool: &PgPool, flake: i32, env: Uuid, hostname: &str, config: Option<&str>) -> Uuid {
    sqlx::query_scalar("INSERT INTO systems(hostname,public_key,derivation,flake_id,environment_id,system_configuration_name) VALUES($1,$1,$1,$2,$3,$4) RETURNING id")
        .bind(hostname).bind(flake).bind(env).bind(config).fetch_one(pool).await.unwrap()
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires isolated migration-backed PostgreSQL"]
async fn scoped_summaries_hide_shared_policy_facts_search_errors_and_revoked_membership(
    pool: PgPool,
) {
    let commit = super::tests::derivation_count_commit(&pool).await;
    let flake: i32 = sqlx::query_scalar("SELECT flake_id FROM commits WHERE id=$1")
        .bind(commit)
        .fetch_one(&pool)
        .await
        .unwrap();
    let user: Uuid = sqlx::query_scalar("INSERT INTO users(username,first_name,last_name,email) VALUES('inventory-viewer','Scope','Reader','inventory@example.invalid') RETURNING id").fetch_one(&pool).await.unwrap();
    let visible: Uuid = sqlx::query_scalar(
        "INSERT INTO environments(name) VALUES('inventory-visible') RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let hidden: Uuid = sqlx::query_scalar(
        "INSERT INTO environments(name) VALUES('inventory-hidden') RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO user_environment_memberships(user_id,environment_id) VALUES($1,$2)")
        .bind(user)
        .bind(visible)
        .execute(&pool)
        .await
        .unwrap();
    // Whitespace normalizes to the shared configuration. Empty configuration
    // falls back to the hostname, matching current matrix authorization.
    let shared_host = host(&pool, flake, visible, "visible-host", Some(" shared ")).await;
    let hidden_shared = host(&pool, flake, hidden, "hidden-shared-host", Some("shared")).await;
    host(&pool, flake, visible, "visible-only", Some("  ")).await;
    host(&pool, flake, hidden, "hidden-only", None).await;
    let other_flake: i32 = sqlx::query_scalar("INSERT INTO flakes(name,repo_url) VALUES('inventory-other','https://example.invalid/inventory-other.git') RETURNING id").fetch_one(&pool).await.unwrap();
    host(
        &pool,
        other_flake,
        visible,
        "other-flake-host",
        Some("hidden-only"),
    )
    .await;
    let names = ["hidden-only", "shared", "unmanaged", "visible-only"];
    // Current registration alone must not grant historical policy authority.
    // This fixture explicitly captures the original host UUID/configuration
    // mapping before discovery, as the production loader does.
    sqlx::query("INSERT INTO evaluation_policy_plans(commit_id,attempt_number,evaluation_attempt_id,payload)
        SELECT c.id,a.attempt_number,a.id,jsonb_build_object(
            'evaluation_attempt_id',a.id,'attempt_number',a.attempt_number,
            'provenance','captured','columns','[]'::jsonb,'bundles','[]'::jsonb,
            'empty_bundle_metadata_complete',true,'rows',
            (SELECT jsonb_agg(jsonb_build_object('system_id',s.id,'system_name',s.hostname,
                'configuration_name',COALESCE(NULLIF(btrim(s.system_configuration_name),''),s.hostname),
                'resolution','resolved','evaluation_digest',NULL,'policies','[]'::jsonb,
                'cells','[]'::jsonb)) FROM systems s WHERE s.flake_id=c.flake_id))
        FROM commits c JOIN evaluation_attempts a ON a.commit_id=c.id
        WHERE c.id=$1 ORDER BY a.attempt_number DESC LIMIT 1")
        .bind(commit).execute(&pool).await.unwrap();
    inventory(&pool, commit, &names, &names).await;
    for name in names {
        native(
            &pool,
            commit,
            name,
            if name == "shared" { "policy" } else { "pass" },
        )
        .await;
    }
    // Hidden malformed legacy evidence must not make visible counts unavailable.
    sqlx::query("UPDATE derivations SET policy_results='{}' WHERE commit_id=$1 AND derivation_name='hidden-only'").bind(commit).execute(&pool).await.unwrap();
    sqlx::query(
        "UPDATE commits SET evaluation_error_message='hidden-only secret failure' WHERE id=$1",
    )
    .bind(commit)
    .execute(&pool)
    .await
    .unwrap();
    assert_summary(
        &pool,
        commit,
        Some(user),
        &["shared", "visible-only"],
        (1, 0, 0, 0, 1, true, true),
    )
    .await;
    let q = list_eval_queue_for_user(&pool, &EvalQueueParams::default(), Some(user))
        .await
        .unwrap();
    assert_eq!(
        q.rows
            .iter()
            .find(|r| r.commit_id == commit)
            .unwrap()
            .derivation_count,
        Some(2)
    );
    let h = list_eval_history_for_user(&pool, &EvalHistoryParams::default(), Some(user))
        .await
        .unwrap();
    assert!(
        h.items
            .iter()
            .find(|r| r.commit_id == commit)
            .unwrap()
            .evaluation_error_message
            .is_none()
    );
    let admin = list_eval_history_for_user(&pool, &EvalHistoryParams::default(), None)
        .await
        .unwrap();
    let row = admin.items.iter().find(|r| r.commit_id == commit).unwrap();
    assert_eq!(row.system_count, 4);
    assert!(
        row.evaluation_error_message
            .as_deref()
            .unwrap()
            .contains("hidden-only")
    );
    // Registering a guessed name after capture cannot reveal whether it was in
    // the historical inventory or make its old gate count visible.
    sqlx::query("INSERT INTO systems(hostname,public_key,derivation,environment_id,flake_id,system_configuration_name)
        SELECT $2,'test-key','',environment_id,flake_id,'unmanaged' FROM systems WHERE id=$1")
        .bind(shared_host).bind(format!("late-authority-{commit}"))
        .execute(&pool).await.unwrap();
    for search in ["hidden-only", "unmanaged"] {
        let q = list_eval_queue_for_user(
            &pool,
            &EvalQueueParams {
                search: Some(search.into()),
                ..Default::default()
            },
            Some(user),
        )
        .await
        .unwrap();
        assert!(!q.rows.iter().any(|r| r.commit_id == commit));
        assert_eq!(q.filtered_total, 0);
        let h = list_eval_history_for_user(
            &pool,
            &EvalHistoryParams {
                search: Some(search.into()),
                ..Default::default()
            },
            Some(user),
        )
        .await
        .unwrap();
        assert!(!h.items.iter().any(|r| r.commit_id == commit));
        assert_eq!(h.total_count, 0);
        assert_eq!(h.domain_total, 1);
        let admin = list_eval_history_for_user(
            &pool,
            &EvalHistoryParams {
                search: Some(search.into()),
                ..Default::default()
            },
            None,
        )
        .await
        .unwrap();
        assert!(admin.items.iter().any(|r| r.commit_id == commit));
    }
    // Removing a hidden captured host cannot grant its historical gate to the
    // remaining visible host; that configuration stays pending.
    sqlx::query("DELETE FROM systems WHERE id=$1")
        .bind(hidden_shared)
        .execute(&pool)
        .await
        .unwrap();
    assert_summary(
        &pool,
        commit,
        Some(user),
        &["shared", "visible-only"],
        (1, 0, 0, 0, 1, true, true),
    )
    .await;
    // Moving a host to a different flake removes its old configuration access.
    sqlx::query("UPDATE systems SET flake_id=NULL WHERE id=$1")
        .bind(shared_host)
        .execute(&pool)
        .await
        .unwrap();
    assert_summary(
        &pool,
        commit,
        Some(user),
        &["visible-only"],
        (1, 0, 0, 0, 0, true, true),
    )
    .await;
    sqlx::query("DELETE FROM user_environment_memberships WHERE user_id=$1")
        .bind(user)
        .execute(&pool)
        .await
        .unwrap();
    let q = list_eval_queue_for_user(&pool, &EvalQueueParams::default(), Some(user))
        .await
        .unwrap();
    assert!(!q.rows.iter().any(|r| r.commit_id == commit));
    assert_eq!(
        (
            q.active_count,
            q.completed_count,
            q.successful_count,
            q.failed_count,
            q.domain_total,
            q.filtered_total
        ),
        (0, 0, 0, 0, 0, 0)
    );
    let h = list_eval_history_for_user(&pool, &EvalHistoryParams::default(), Some(user))
        .await
        .unwrap();
    assert!(h.items.is_empty());
    assert_eq!((h.domain_total, h.total_count), (0, 0));
}
