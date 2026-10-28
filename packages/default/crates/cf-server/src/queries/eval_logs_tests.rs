//! Exact-attempt persistence regressions. Every database is SQLx-owned.

use crate::queries::commits::{self, EvalStartOutcome};
use crate::queries::eval_logs::{self, EvalLogOwner};
use sqlx::PgPool;
use uuid::Uuid;

async fn fixture(pool: &PgPool) -> EvalLogOwner {
    let suffix = Uuid::new_v4().to_string();
    let flake: i32 =
        sqlx::query_scalar("INSERT INTO flakes(name, repo_url) VALUES ($1, $2) RETURNING id")
            .bind(&suffix)
            .bind(format!("https://example.invalid/{suffix}"))
            .fetch_one(pool)
            .await
            .unwrap();
    let commit: i32 = sqlx::query_scalar(
        "INSERT INTO commits(flake_id, git_commit_hash, commit_timestamp, evaluation_status)
         VALUES ($1, $2, NOW(), 'pending') RETURNING id",
    )
    .bind(flake)
    .bind(&suffix)
    .fetch_one(pool)
    .await
    .unwrap();
    assert!(matches!(
        commits::mark_commit_evaluation_started(pool, commit)
            .await
            .unwrap(),
        EvalStartOutcome::Started { attempt: 1 }
    ));
    let mut tx = pool.begin().await.unwrap();
    let owner = eval_logs::claimed_owner_in_tx(&mut tx, commit, 1)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    owner
}

async fn successor(pool: &PgPool, a: EvalLogOwner) -> EvalLogOwner {
    sqlx::query("UPDATE automatic_retry_policy SET max_evaluation_retries = 0 WHERE id = 1")
        .execute(pool)
        .await
        .unwrap();
    commits::mark_commit_evaluation_failed(
        pool,
        a.commit_id,
        "test failure",
        a.attempt_number,
        crate::models::retry_policy::RetryFailureClass::Deterministic,
    )
    .await
    .unwrap();
    commits::reset_commit_evaluation(pool, a.commit_id)
        .await
        .unwrap();
    assert!(
        matches!(commits::mark_commit_evaluation_started(pool, a.commit_id).await.unwrap(),
        EvalStartOutcome::Started { attempt } if attempt == a.attempt_number + 1)
    );
    let mut tx = pool.begin().await.unwrap();
    let owner = eval_logs::claimed_owner_in_tx(&mut tx, a.commit_id, a.attempt_number + 1)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    owner
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires isolated migration-backed PostgreSQL"]
async fn attempt_owned_logs_retain_history_and_fence_stale_single_and_batch(pool: PgPool) {
    let a = fixture(&pool).await;
    eval_logs::insert_eval_log(&pool, a, 1, Some("info"), "A-private")
        .await
        .unwrap();
    // Legacy storage remains valid, but no current-attempt reader selects it.
    sqlx::query("INSERT INTO eval_logs(commit_id, log_sequence, log_message) VALUES ($1, 1, 'legacy-private')")
        .bind(a.commit_id).execute(&pool).await.unwrap();
    let b = successor(&pool, a).await;
    assert_ne!(a.evaluation_attempt_id, b.evaluation_attempt_id);
    assert!(matches!(
        eval_logs::insert_eval_log(&pool, a, 2, None, "A-late").await,
        Err(sqlx::Error::RowNotFound)
    ));
    assert!(matches!(
        eval_logs::insert_eval_logs_batch(
            &pool,
            a,
            &[
                (3, None, "A-stderr".into()),
                (4, None, "A-final-flush".into())
            ]
        )
        .await,
        Err(sqlx::Error::RowNotFound)
    ));
    assert_eq!(
        eval_logs::insert_eval_logs_batch(
            &pool,
            b,
            &[(1, None, "B-visible".into()), (2, None, "B-stderr".into())]
        )
        .await
        .unwrap(),
        2
    );
    assert_eq!(
        eval_logs::insert_eval_logs_batch(
            &pool,
            b,
            &[(1, None, "duplicate-must-not-replace".into())]
        )
        .await
        .unwrap(),
        0
    );
    let mut tx = pool.begin().await.unwrap();
    let a_rows = eval_logs::fetch_eval_logs_by_commit_in_tx(&mut tx, a)
        .await
        .unwrap();
    let b_rows = eval_logs::fetch_eval_logs_by_commit_in_tx(&mut tx, b)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(a_rows.len(), 1);
    assert_eq!(a_rows[0].log_message, "A-private");
    assert_eq!(b_rows.len(), 2);
    assert_eq!(b_rows[0].log_message, "B-visible");
    assert_eq!(b_rows[1].log_message, "B-stderr");
    let all = eval_logs::fetch_eval_logs_by_commit(&pool, a.commit_id)
        .await
        .unwrap();
    assert_eq!(all.len(), 4, "source and unattributed storage are retained");
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires isolated migration-backed PostgreSQL"]
async fn attempt_log_constraints_reject_partial_and_foreign_attribution(pool: PgPool) {
    let owner = fixture(&pool).await;
    for (id, number) in [
        (Some(owner.evaluation_attempt_id), None),
        (None, Some(owner.attempt_number)),
        (Some(owner.evaluation_attempt_id), Some(0)),
        (Some(Uuid::new_v4()), Some(owner.attempt_number)),
        (
            Some(owner.evaluation_attempt_id),
            Some(owner.attempt_number + 1),
        ),
    ] {
        assert!(
            sqlx::query(
                "INSERT INTO eval_logs(commit_id, evaluation_attempt_id, attempt_number,
                 log_sequence, log_message) VALUES ($1, $2, $3, 1, 'invalid')",
            )
            .bind(owner.commit_id)
            .bind(id)
            .bind(number)
            .execute(&pool)
            .await
            .is_err()
        );
    }
    let foreign = EvalLogOwner {
        commit_id: owner.commit_id + 1000000,
        ..owner
    };
    assert!(
        eval_logs::insert_eval_log(&pool, foreign, 1, None, "foreign")
            .await
            .is_err()
    );
    let foreign_uuid = EvalLogOwner {
        evaluation_attempt_id: Uuid::new_v4(),
        ..owner
    };
    assert!(
        eval_logs::insert_eval_logs_batch(&pool, foreign_uuid, &[(1, None, "foreign".into())])
            .await
            .is_err()
    );
}
