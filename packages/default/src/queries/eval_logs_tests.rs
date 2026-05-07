use crate::queries::commits::{get_commit_by_hash, insert_commit_with_metadata};
use crate::queries::eval_logs::{
    delete_eval_logs_by_commit, fetch_eval_logs_by_commit, insert_eval_log,
};
use crate::queries::flakes::insert_flake;
use chrono::Utc;
use sqlx::PgPool;

async fn test_pool_from_env() -> PgPool {
    let db_url =
        std::env::var("DATABASE_URL").expect("DATABASE_URL must be set for eval log DB tests");

    PgPool::connect(&db_url)
        .await
        .expect("failed to connect to DATABASE_URL")
}

#[tokio::test]
#[ignore = "requires live database connection"]
async fn re_evaluation_replaces_persisted_logs_for_same_commit() {
    let pool = test_pool_from_env().await;

    let suffix = Utc::now()
        .timestamp_nanos_opt()
        .expect("timestamp_nanos should be available")
        .to_string();

    let flake_name = format!("task-289-eval-logs-{suffix}");
    let flake_url = format!("https://example.com/{flake_name}.git");
    let commit_hash = format!("task289reval{suffix}");

    let flake = insert_flake(
        &pool,
        &flake_name,
        &flake_url,
        "main",
        "cf_systems_only",
    )
    .await
    .expect("insert_flake should succeed");

    insert_commit_with_metadata(
        &pool,
        &commit_hash,
        &flake.repo_url,
        Utc::now(),
        Some("task-289 re-eval log test"),
        Some("test"),
    )
    .await
    .expect("insert_commit_with_metadata should succeed");

    let commit = get_commit_by_hash(&pool, &commit_hash)
        .await
        .expect("get_commit_by_hash should succeed");

    // Simulate first evaluation attempt
    insert_eval_log(&pool, commit.id, 1, Some("info"), "first attempt line 1")
        .await
        .expect("insert first attempt line 1");
    insert_eval_log(&pool, commit.id, 2, Some("warn"), "first attempt line 2")
        .await
        .expect("insert first attempt line 2");

    let first_attempt_logs = fetch_eval_logs_by_commit(&pool, commit.id)
        .await
        .expect("fetch first attempt logs should succeed");
    assert_eq!(first_attempt_logs.len(), 2);

    // Simulate re-evaluation start behavior: clear old logs then write new sequence from 1
    let deleted = delete_eval_logs_by_commit(&pool, commit.id)
        .await
        .expect("delete_eval_logs_by_commit should succeed");
    assert_eq!(deleted, 2);

    insert_eval_log(&pool, commit.id, 1, Some("info"), "second attempt line 1")
        .await
        .expect("insert second attempt line 1");
    insert_eval_log(&pool, commit.id, 2, Some("error"), "second attempt line 2")
        .await
        .expect("insert second attempt line 2");

    let second_attempt_logs = fetch_eval_logs_by_commit(&pool, commit.id)
        .await
        .expect("fetch second attempt logs should succeed");

    assert_eq!(second_attempt_logs.len(), 2);
    assert_eq!(second_attempt_logs[0].log_sequence, 1);
    assert_eq!(second_attempt_logs[1].log_sequence, 2);
    assert_eq!(second_attempt_logs[0].log_message, "second attempt line 1");
    assert_eq!(second_attempt_logs[1].log_message, "second attempt line 2");
}
