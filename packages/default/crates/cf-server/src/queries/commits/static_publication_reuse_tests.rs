//! Exact-reuse admission with static cache publications (migration 0328).
//!
//! Each case varies one fact about the origin's publication or about the cache
//! configuration in force, then checks the two admission points that depend on
//! them: adoption of a successful build job, and admission of a scan of a reused
//! head. The origin build is the same in every case, so a difference in outcome
//! comes from the publication fact alone.

use super::derivation_reuse_tests::{OUT, carrier, evaluate, finish_evaluation, registration};
use crate::queries::build_jobs::{self, BuildJobInsertOutcome};
use crate::queries::cve_scans::{SystemScanAdmission, admit_system_cve_scan_with_static};
use sqlx::PgPool;
use uuid::Uuid;

const STATIC_CACHE: &str = "ssh-ng://cache@static.example";

/// One origin A with a real successful build job and no publication record,
/// and a documentation-only head B that has not adopted anything yet.
struct Fixture {
    flake: i32,
    origin: i32,
    binding: i32,
    job: Uuid,
}

async fn fixture(pool: &PgPool) -> Fixture {
    let (flake, _) = registration(pool).await;
    let (a, _, _, _, _) = evaluate(pool, flake, 'a', "host", true).await;
    let origin = carrier(pool, a).await;
    let job = {
        let mut tx = pool.begin().await.unwrap();
        let admitted = build_jobs::create_build_job_for_derivation_tx(&mut tx, origin)
            .await
            .unwrap();
        tx.commit().await.unwrap();
        match admitted {
            Some(BuildJobInsertOutcome::Inserted { build_job_id }) => build_job_id,
            _ => panic!("the origin needs one real build job"),
        }
    };
    sqlx::query("UPDATE build_jobs SET status='success',completed_at=now() WHERE id=$1")
        .bind(job)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("UPDATE derivations SET status_id=10,store_path=$2,completed_at=now(),build_preparation_state='queued' WHERE id=$1")
        .bind(origin)
        .bind(OUT)
        .execute(pool)
        .await
        .unwrap();
    finish_evaluation(pool, a).await;
    // B is evaluated with the identical drv. Its physical alias to A is fixed by
    // evaluation; adoption of A's job is not, because A has no publication yet.
    let (b, _, _, _, _) = evaluate(pool, flake, 'b', "host", true).await;
    let binding = carrier(pool, b).await;
    let alias: (Option<i32>, Option<Uuid>) = sqlx::query_as(
        "SELECT reused_from_derivation_id,reused_build_job_id FROM derivations WHERE id=$1",
    )
    .bind(binding)
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(
        alias,
        (Some(origin), None),
        "B aliases A physically and has not adopted A's job"
    );
    Fixture {
        flake,
        origin,
        binding,
        job,
    }
}

/// Records one publication row of the origin. `source` is the provenance column.
async fn publish(
    pool: &PgPool,
    origin: i32,
    source: &str,
    destination: &str,
    destination_id: Option<i32>,
    status: &str,
    output: &str,
) {
    sqlx::query(
        "INSERT INTO cache_push_jobs(derivation_id,store_path,status,completed_at,
            cache_destination,cache_destination_id,cache_destination_source)
         VALUES($1,$2,$3,CASE WHEN $3='completed' THEN now() END,$4,$5,$6)",
    )
    .bind(origin)
    .bind(output)
    .bind(status)
    .bind(destination)
    .bind(destination_id)
    .bind(source)
    .execute(pool)
    .await
    .unwrap();
}

async fn database_destination(pool: &PgPool, name: &str, push_to: &str, enabled: bool) -> i32 {
    sqlx::query_scalar(
        "INSERT INTO cache_destinations(name,cache_type,enabled,push_to) VALUES($1,'Nix',$2,$3) RETURNING id",
    )
    .bind(name)
    .bind(enabled)
    .bind(push_to)
    .fetch_one(pool)
    .await
    .unwrap()
}

/// Returns the job that B would adopt under the given static configuration.
async fn adopted(pool: &PgPool, binding: i32, static_destination: Option<&str>) -> Option<Uuid> {
    let mut tx = pool.begin().await.unwrap();
    let found =
        build_jobs::reusable_physical_build_with_static_tx(&mut tx, binding, static_destination)
            .await
            .unwrap();
    tx.rollback().await.unwrap();
    found.map(|(job, status)| {
        assert_eq!(status, "success");
        job
    })
}

/// Reports whether a scan of reused head B passes the publication gate.
/// `Ineligible` is that gate; the other outcomes come after it.
async fn scan_admitted(pool: &PgPool, f: &Fixture, static_destination: Option<&str>) -> bool {
    // Bind the head and its carrier so the scoped scan request resolves to B.
    let binding_alias = sqlx::query_scalar::<_, Option<i32>>(
        "SELECT reused_from_derivation_id FROM derivations WHERE id=$1",
    )
    .bind(f.binding)
    .fetch_one(pool)
    .await
    .unwrap();
    assert!(
        binding_alias.is_some(),
        "the scan case needs an adopted alias"
    );
    // B is the registered branch head, which is what an external scan selects.
    let head_commit: i32 = sqlx::query_scalar("SELECT commit_id FROM derivations WHERE id=$1")
        .bind(f.binding)
        .fetch_one(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO flake_branch_commit_snapshot(flake_id,commit_id,position) VALUES($1,$2,0) ON CONFLICT DO NOTHING")
        .bind(f.flake)
        .bind(head_commit)
        .execute(pool)
        .await
        .unwrap();
    let external: Uuid = sqlx::query_scalar(
        "INSERT INTO systems(hostname,system_configuration_name,flake_id,management_mode,derivation,deployment_policy)
         VALUES('static-reuse-external','host',$1,'external','','manual')
         ON CONFLICT (hostname) DO UPDATE SET hostname=EXCLUDED.hostname RETURNING id",
    )
    .bind(f.flake)
    .fetch_one(pool)
    .await
    .unwrap();
    let actor: Uuid = sqlx::query_scalar(
        "INSERT INTO users(username,first_name,last_name,email,user_type)
         VALUES('static-reuse-admin','Static','Admin','static-reuse@example.invalid','human')
         ON CONFLICT (username) DO UPDATE SET username=EXCLUDED.username RETURNING id",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO user_role_assignments(user_id,role) VALUES($1,'admin') ON CONFLICT DO NOTHING",
    )
    .bind(actor)
    .execute(pool)
    .await
    .unwrap();
    let outcome = admit_system_cve_scan_with_static(
        pool,
        actor,
        external,
        Some(f.binding),
        false,
        None,
        None,
        static_destination,
    )
    .await
    .unwrap();
    match outcome {
        SystemScanAdmission::Ineligible(reason) => {
            eprintln!("scan refused: {reason}");
            false
        }
        _ => true,
    }
}

/// Makes B an adopted alias of A so the scan case can resolve the execution source.
async fn adopt(pool: &PgPool, f: &Fixture) {
    sqlx::query(
        "UPDATE derivations SET reused_from_derivation_id=$2,reused_build_job_id=$3 WHERE id=$1",
    )
    .bind(f.binding)
    .bind(f.origin)
    .bind(f.job)
    .execute(pool)
    .await
    .unwrap();
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires owner-run isolated PostgreSQL"]
async fn static_publication_is_admitted_only_for_the_effective_static_destination(pool: PgPool) {
    let f = fixture(&pool).await;
    // No publication at all: nothing is adoptable, whatever the configuration.
    assert_eq!(adopted(&pool, f.binding, Some(STATIC_CACHE)).await, None);

    publish(
        &pool,
        f.origin,
        "static",
        STATIC_CACHE,
        None,
        "completed",
        OUT,
    )
    .await;
    // Positive: the exact output, completed, to the destination in force.
    assert_eq!(
        adopted(&pool, f.binding, Some(STATIC_CACHE)).await,
        Some(f.job)
    );
    // Publication disabled (push_after_build false) or unconfigured: not admitted.
    assert_eq!(adopted(&pool, f.binding, None).await, None);
    assert_eq!(adopted(&pool, f.binding, Some("")).await, None);
    assert_eq!(adopted(&pool, f.binding, Some("   ")).await, None);
    // Changed destination: the recorded one is no longer the configured one.
    assert_eq!(
        adopted(&pool, f.binding, Some("ssh-ng://cache@moved.example")).await,
        None
    );
    assert_eq!(
        adopted(&pool, f.binding, Some("ssh-ng://cache@static.example/")).await,
        None,
        "destination equality is exact"
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires owner-run isolated PostgreSQL"]
async fn static_publication_requires_a_completed_publication_of_the_exact_output(pool: PgPool) {
    let f = fixture(&pool).await;
    // Failed, queued and in-progress publications prove nothing.
    // The schema allows one pending or in-progress row per derivation, so the
    // single non-terminal row moves through its states.
    publish(&pool, f.origin, "static", STATIC_CACHE, None, "failed", OUT).await;
    assert_eq!(adopted(&pool, f.binding, Some(STATIC_CACHE)).await, None);
    publish(
        &pool,
        f.origin,
        "static",
        STATIC_CACHE,
        None,
        "pending",
        OUT,
    )
    .await;
    assert_eq!(adopted(&pool, f.binding, Some(STATIC_CACHE)).await, None);
    sqlx::query("UPDATE cache_push_jobs SET status='in_progress' WHERE derivation_id=$1 AND status='pending'")
        .bind(f.origin)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(adopted(&pool, f.binding, Some(STATIC_CACHE)).await, None);
    sqlx::query(
        "DELETE FROM cache_push_jobs WHERE derivation_id=$1 AND status IN ('failed','in_progress')",
    )
    .bind(f.origin)
    .execute(&pool)
    .await
    .unwrap();
    // A completed publication of a different output does not cover this output.
    let other_output = format!("/nix/store/{}-other", "c".repeat(32));
    publish(
        &pool,
        f.origin,
        "static",
        STATIC_CACHE,
        None,
        "completed",
        &other_output,
    )
    .await;
    assert_eq!(adopted(&pool, f.binding, Some(STATIC_CACHE)).await, None);
    // A completed publication of another derivation's output is not this one's.
    // Evaluation is serial per flake, so B's evaluation is finished first.
    let b_commit: i32 = sqlx::query_scalar("SELECT commit_id FROM derivations WHERE id=$1")
        .bind(f.binding)
        .fetch_one(&pool)
        .await
        .unwrap();
    finish_evaluation(&pool, b_commit).await;
    let (elsewhere, _, _, _, _) = evaluate(&pool, f.flake, 'd', "unrelated", true).await;
    let unrelated = carrier(&pool, elsewhere).await;
    publish(
        &pool,
        unrelated,
        "static",
        STATIC_CACHE,
        None,
        "completed",
        OUT,
    )
    .await;
    assert_eq!(adopted(&pool, f.binding, Some(STATIC_CACHE)).await, None);
    // The exact completed publication finally qualifies.
    publish(
        &pool,
        f.origin,
        "static",
        STATIC_CACHE,
        None,
        "completed",
        OUT,
    )
    .await;
    assert_eq!(
        adopted(&pool, f.binding, Some(STATIC_CACHE)).await,
        Some(f.job)
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires owner-run isolated PostgreSQL"]
async fn database_and_legacy_publication_rules_are_unchanged(pool: PgPool) {
    let f = fixture(&pool).await;
    // A database publication is admitted by the database rules alone: with no
    // static configuration, and with an unrelated one.
    let id = database_destination(&pool, "db-cache", "ssh-ng://cache@db.example", true).await;
    publish(
        &pool,
        f.origin,
        "database",
        "db-cache",
        Some(id),
        "completed",
        OUT,
    )
    .await;
    for configured in [None, Some(STATIC_CACHE)] {
        assert_eq!(adopted(&pool, f.binding, configured).await, Some(f.job));
    }
    // Disabling the destination removes eligibility. A static configuration that
    // happens to name the same URL must not stand in for it.
    sqlx::query("UPDATE cache_destinations SET enabled=false WHERE id=$1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(adopted(&pool, f.binding, None).await, None);
    assert_eq!(
        adopted(&pool, f.binding, Some("ssh-ng://cache@db.example")).await,
        None,
        "a database publication is never converted into a static one"
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires owner-run isolated PostgreSQL"]
async fn legacy_publication_identity_must_stay_unambiguous(pool: PgPool) {
    let f = fixture(&pool).await;
    database_destination(&pool, "legacy-cache", "ssh-ng://cache@legacy.example", true).await;
    publish(
        &pool,
        f.origin,
        "legacy",
        "legacy-cache",
        None,
        "completed",
        OUT,
    )
    .await;
    // One matching destination: admitted, with or without static configuration.
    assert_eq!(adopted(&pool, f.binding, None).await, Some(f.job));
    assert_eq!(
        adopted(&pool, f.binding, Some(STATIC_CACHE)).await,
        Some(f.job)
    );
    // A second destination answering to the same name makes the identity
    // ambiguous. Static configuration must not rescue it.
    database_destination(&pool, "other", "legacy-cache", false).await;
    assert_eq!(adopted(&pool, f.binding, None).await, None);
    assert_eq!(adopted(&pool, f.binding, Some(STATIC_CACHE)).await, None);
    assert_eq!(
        adopted(&pool, f.binding, Some("legacy-cache")).await,
        None,
        "a static destination equal to a legacy reference does not resolve it"
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires owner-run isolated PostgreSQL"]
async fn scan_of_a_reused_head_fails_closed_without_a_matching_static_publication(pool: PgPool) {
    let f = fixture(&pool).await;
    adopt(&pool, &f).await;
    // Without any publication the reused head is not scannable.
    assert!(!scan_admitted(&pool, &f, Some(STATIC_CACHE)).await);
    publish(
        &pool,
        f.origin,
        "static",
        STATIC_CACHE,
        None,
        "completed",
        OUT,
    )
    .await;
    // Matching configuration: admitted past the publication gate.
    assert!(scan_admitted(&pool, &f, Some(STATIC_CACHE)).await);
    // Disabled, changed or unreadable configuration: refused.
    assert!(!scan_admitted(&pool, &f, None).await);
    assert!(!scan_admitted(&pool, &f, Some("ssh-ng://cache@moved.example")).await);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires owner-run isolated PostgreSQL"]
async fn a_static_publication_never_resolves_a_delivery_source(pool: PgPool) {
    // Admission accepts the static publication, but delivery to an agent resolves
    // sources only from database destinations. Even a database destination with
    // the same URL must not receive the static record's credentials.
    let f = fixture(&pool).await;
    adopt(&pool, &f).await;
    publish(
        &pool,
        f.origin,
        "static",
        STATIC_CACHE,
        None,
        "completed",
        OUT,
    )
    .await;
    database_destination(&pool, "colliding", STATIC_CACHE, true).await;
    assert_eq!(
        adopted(&pool, f.binding, Some(STATIC_CACHE)).await,
        Some(f.job)
    );
    let mut tx = pool.begin().await.unwrap();
    let read = crate::queries::cache_publication_reads::resolve_publication_read_tx(
        &mut tx,
        f.binding,
        OUT,
        None,
        true,
        cf_protocol::agent::AgentCapabilities::default(),
    )
    .await
    .unwrap();
    tx.rollback().await.unwrap();
    assert!(
        read.is_none(),
        "a static publication yields no delivery read source or credential"
    );
}

/// Removing a flake removes an origin, its alias and the job the alias adopted.
/// The alias references the origin and the job with RESTRICT foreign keys, so the
/// cascade must not trip over its own ordering.
#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires owner-run isolated PostgreSQL"]
async fn flake_deletion_removes_origin_alias_and_adopted_job_together(pool: PgPool) {
    let f = fixture(&pool).await;
    adopt(&pool, &f).await;
    let before: (i64, i64) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM derivations WHERE reused_build_job_id IS NOT NULL),
                (SELECT count(*) FROM build_jobs)",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(before, (1, 1), "an alias adopted the origin's real job");
    let mut tx = pool.begin().await.unwrap();
    crate::queries::flakes::cascade_delete_flake(&mut tx, f.flake)
        .await
        .expect("a flake with an adopted job must be deletable");
    tx.commit().await.unwrap();
    let after: (i64, i64, i64) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM derivations), (SELECT count(*) FROM build_jobs),
                (SELECT count(*) FROM commits)",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(after, (0, 0, 0));
}
