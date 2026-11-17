//! Isolated PostgreSQL tests for the per-system flake-position read.
//! Runs in the isolated CI database path (`packages/devScripts/tests/database-regressions.sh`).

use crystal_forge::api::models::{
    FlakePositionBuildState, FlakePositionOrdering, FlakePositionUnknownReason,
};
use crystal_forge::queries::flake_position::fetch_system_flake_position;
use sqlx::PgPool;
use uuid::Uuid;

/// Five branch commits, HEAD first. Index `i` has SHA `sha(i)`.
fn sha(index: usize) -> String {
    format!("{}{index}", "a".repeat(39))
}

struct Fleet {
    flake: i32,
    commits: Vec<i32>,
    env_a: Uuid,
    env_b: Uuid,
}

async fn fleet(pool: &PgPool, snapshot: bool) -> Fleet {
    let flake: i32 = sqlx::query_scalar(
        "INSERT INTO flakes(name,repo_url,branch) VALUES('lane','https://example.invalid/lane','main') RETURNING id",
    )
    .fetch_one(pool)
    .await
    .expect("flake");
    let mut commits = Vec::new();
    for index in 0..5_usize {
        // Timestamps are reversed against branch order so the tests prove
        // which ordering the read uses.
        let id: i32 = sqlx::query_scalar(
            "INSERT INTO commits(flake_id,git_commit_hash,commit_timestamp,message,author) \
             VALUES($1,$2,now() - make_interval(hours => $3),$4,'dev') RETURNING id",
        )
        .bind(flake)
        .bind(sha(index))
        .bind(if snapshot {
            10 - index as i32
        } else {
            index as i32
        })
        .bind(format!("commit {index}"))
        .fetch_one(pool)
        .await
        .expect("commit");
        if snapshot {
            sqlx::query(
                "INSERT INTO flake_branch_commit_snapshot(flake_id,commit_id,position) VALUES($1,$2,$3)",
            )
            .bind(flake)
            .bind(id)
            .bind(index as i32)
            .execute(pool)
            .await
            .expect("snapshot");
        }
        commits.push(id);
    }
    if snapshot {
        sqlx::query("UPDATE flakes SET snapshot_ready_at=now() WHERE id=$1")
            .bind(flake)
            .execute(pool)
            .await
            .expect("snapshot ready");
    }
    let env_a = sqlx::query_scalar("INSERT INTO environments(name) VALUES('lane-a') RETURNING id")
        .fetch_one(pool)
        .await
        .expect("environment a");
    let env_b = sqlx::query_scalar("INSERT INTO environments(name) VALUES('lane-b') RETURNING id")
        .fetch_one(pool)
        .await
        .expect("environment b");
    Fleet {
        flake,
        commits,
        env_a,
        env_b,
    }
}

/// Registers `hostname` and, when `running` is set, reports the output of
/// that commit index as its running store path.
async fn system(
    pool: &PgPool,
    fleet: &Fleet,
    hostname: &str,
    environment: Uuid,
    running: Option<usize>,
) -> Uuid {
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO systems(hostname,public_key,derivation,flake_id,system_configuration_name,environment_id) \
         VALUES($1,'test-key','test-derivation',$2,$1,$3) RETURNING id",
    )
    .bind(hostname)
    .bind(fleet.flake)
    .bind(environment)
    .fetch_one(pool)
    .await
    .expect("system");
    for (index, commit) in fleet.commits.iter().enumerate() {
        sqlx::query(
            "INSERT INTO derivations(commit_id,derivation_type,derivation_name,derivation_path,status_id,completed_at,store_path) \
             VALUES($1,'nixos',$2,$3,10,now(),$4)",
        )
        .bind(commit)
        .bind(hostname)
        .bind(format!("/nix/store/{hostname}-{index}.drv"))
        .bind(format!("/nix/store/{hostname}-{index}"))
        .execute(pool)
        .await
        .expect("derivation");
    }
    if let Some(index) = running {
        sqlx::query(
            "INSERT INTO system_states(hostname,store_path,change_reason) VALUES($1,$2,'startup')",
        )
        .bind(hostname)
        .bind(format!("/nix/store/{hostname}-{index}"))
        .execute(pool)
        .await
        .expect("state");
    }
    id
}

async fn viewer_in(pool: &PgPool, environment: Uuid) -> Uuid {
    let user: Uuid = sqlx::query_scalar(
        "INSERT INTO users(username,first_name,last_name,email) VALUES('lane-viewer','Lane','Viewer','lane@example.invalid') RETURNING id",
    )
    .fetch_one(pool)
    .await
    .expect("user");
    sqlx::query("INSERT INTO user_environment_memberships(user_id,environment_id) VALUES($1,$2)")
        .bind(user)
        .bind(environment)
        .execute(pool)
        .await
        .expect("membership");
    user
}

#[sqlx::test(migrations = "./migrations")]
async fn behind_position_peers_and_build_state(pool: PgPool) {
    let fleet = fleet(&pool, true).await;
    let subject = system(&pool, &fleet, "lane-subject", fleet.env_a, Some(2)).await;
    system(&pool, &fleet, "lane-peer-a", fleet.env_a, Some(2)).await;
    system(&pool, &fleet, "lane-peer-b", fleet.env_b, Some(0)).await;
    sqlx::query(
        "INSERT INTO build_jobs(derivation_id,status) \
         SELECT id,'queued' FROM derivations WHERE derivation_name='lane-subject' AND derivation_path LIKE '%-0.drv'",
    )
    .execute(&pool)
    .await
    .expect("queued build");
    // Commit 3: a success followed by a queued retry, with its output pushed to
    // the cache, still reads as ready.
    sqlx::query(
        "INSERT INTO build_jobs(derivation_id,status) \
         SELECT id, s FROM derivations, unnest(ARRAY['success','queued']) s \
         WHERE derivation_name='lane-subject' AND derivation_path LIKE '%-3.drv'",
    )
    .execute(&pool)
    .await
    .expect("success then retry");
    sqlx::query(
        "INSERT INTO cache_push_jobs(derivation_id,status,store_path) \
         SELECT id,'completed',store_path FROM derivations \
         WHERE derivation_name='lane-subject' AND derivation_path LIKE '%-3.drv'",
    )
    .execute(&pool)
    .await
    .expect("published");
    // Commit 4: built, but the cache push never completed (TASK-527).
    sqlx::query(
        "INSERT INTO build_jobs(derivation_id,status) \
         SELECT id,'success' FROM derivations \
         WHERE derivation_name='lane-subject' AND derivation_path LIKE '%-4.drv'",
    )
    .execute(&pool)
    .await
    .expect("built");
    sqlx::query(
        "INSERT INTO cache_push_jobs(derivation_id,status,store_path,error_message) \
         SELECT id,'pending',store_path,'cache_probe_failed' FROM derivations \
         WHERE derivation_name='lane-subject' AND derivation_path LIKE '%-4.drv'",
    )
    .execute(&pool)
    .await
    .expect("pending publication");

    let admin_view = fetch_system_flake_position(&pool, subject, None)
        .await
        .expect("read")
        .expect("system exists");
    assert_eq!(admin_view.ordering, FlakePositionOrdering::BranchSnapshot);
    assert_eq!(
        admin_view
            .commits
            .iter()
            .map(|c| c.sha.clone())
            .collect::<Vec<_>>(),
        (0..5).map(sha).collect::<Vec<_>>(),
        "snapshot order wins over commit timestamps"
    );
    assert_eq!(admin_view.running_commit.as_deref(), Some(sha(2).as_str()));
    assert_eq!(admin_view.running_position, Some(2));
    assert_eq!(admin_view.unknown_reason, None);
    assert_eq!(admin_view.last_tracked_position, Some(2));
    assert_eq!(
        admin_view.commits[0].build_state,
        FlakePositionBuildState::Building
    );
    assert_eq!(
        admin_view.commits[1].build_state,
        FlakePositionBuildState::NotBuilt
    );
    assert_eq!(
        admin_view.commits[3].build_state,
        FlakePositionBuildState::Ready
    );
    assert_eq!(
        admin_view.commits[4].build_state,
        FlakePositionBuildState::Unpublished
    );
    assert_eq!(admin_view.commits[2].peers, vec!["lane-peer-a".to_string()]);
    assert_eq!(admin_view.commits[0].peers, vec!["lane-peer-b".to_string()]);

    // SECURITY: A viewer in environment A does not learn hostnames from B.
    let viewer = viewer_in(&pool, fleet.env_a).await;
    let scoped = fetch_system_flake_position(&pool, subject, Some(viewer))
        .await
        .expect("read")
        .expect("system exists");
    assert_eq!(scoped.commits[2].peers, vec!["lane-peer-a".to_string()]);
    assert!(scoped.commits[0].peers.is_empty());
}

#[sqlx::test(migrations = "./migrations")]
async fn timestamp_fallback_and_unknown_reasons(pool: PgPool) {
    let fleet = fleet(&pool, false).await;
    let unobserved = system(&pool, &fleet, "lane-quiet", fleet.env_a, None).await;
    let untracked = system(&pool, &fleet, "lane-local", fleet.env_a, None).await;
    sqlx::query(
        "INSERT INTO system_states(hostname,store_path,change_reason) VALUES('lane-local','/nix/store/local-rebuild','config_change')",
    )
    .execute(&pool)
    .await
    .expect("untracked state");

    let quiet = fetch_system_flake_position(&pool, unobserved, None)
        .await
        .expect("read")
        .expect("system exists");
    assert_eq!(quiet.ordering, FlakePositionOrdering::CommitTimestamp);
    assert_eq!(quiet.commits[0].sha, sha(0), "newest timestamp first");
    assert_eq!(
        quiet.unknown_reason,
        Some(FlakePositionUnknownReason::NotObserved)
    );
    assert_eq!(quiet.running_position, None);

    let local = fetch_system_flake_position(&pool, untracked, None)
        .await
        .expect("read")
        .expect("system exists");
    assert_eq!(
        local.unknown_reason,
        Some(FlakePositionUnknownReason::Untracked)
    );

    assert!(
        fetch_system_flake_position(&pool, Uuid::new_v4(), None)
            .await
            .expect("read")
            .is_none(),
        "a missing system has no position"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn shared_output_places_the_host_at_the_newest_matching_commit(pool: PgPool) {
    // Commits 0 (HEAD) and 2 build the same closure for this host, as happens
    // when newer commits do not touch its configuration.
    let fleet = fleet(&pool, true).await;
    let subject = system(&pool, &fleet, "lane-shared", fleet.env_a, Some(2)).await;
    sqlx::query(
        "UPDATE derivations SET store_path='/nix/store/lane-shared-2' \
         WHERE derivation_name='lane-shared' AND derivation_path LIKE '%-0.drv'",
    )
    .execute(&pool)
    .await
    .expect("shared output");

    let view = fetch_system_flake_position(&pool, subject, None)
        .await
        .expect("read")
        .expect("system exists");
    assert_eq!(
        view.running_position,
        Some(0),
        "same output as HEAD means on HEAD"
    );
    assert_eq!(view.running_commit.as_deref(), Some(sha(0).as_str()));
    assert_eq!(view.unknown_reason, None);
}

#[sqlx::test(migrations = "./migrations")]
async fn deployment_progress_labels_the_requested_or_newest_commit(pool: PgPool) {
    use crystal_forge::queries::system_events::get_system_deployment_progress_row;
    // Commits 0 (HEAD) and 2 share one output; the host runs commit 2.
    let fleet = fleet(&pool, true).await;
    let subject = system(&pool, &fleet, "lane-label", fleet.env_a, Some(2)).await;
    sqlx::query(
        "UPDATE derivations SET store_path='/nix/store/lane-label-2' \
         WHERE derivation_name='lane-label' AND derivation_path LIKE '%-0.drv'",
    )
    .execute(&pool)
    .await
    .expect("shared output");
    let insert = |requested: Option<i32>| {
        sqlx::query(
            "INSERT INTO pending_system_deployments(system_id,target_store_path,status,source,expires_at,requested_commit_id) \
             VALUES($1,'/nix/store/lane-label-2','pending','manual',now() + interval '2 hours',$2)",
        )
        .bind(subject)
        .bind(requested)
    };

    insert(None).execute(&pool).await.expect("unrequested");
    let row = get_system_deployment_progress_row(&pool, subject)
        .await
        .expect("read")
        .expect("row");
    assert_eq!(
        row.target_commit.as_deref(),
        Some(sha(0).as_str()),
        "newest commit with the output"
    );

    sqlx::query("UPDATE pending_system_deployments SET status='superseded', completed_at=now() WHERE system_id=$1")
        .bind(subject)
        .execute(&pool)
        .await
        .expect("supersede");
    insert(Some(fleet.commits[2]))
        .execute(&pool)
        .await
        .expect("requested");
    let row = get_system_deployment_progress_row(&pool, subject)
        .await
        .expect("read")
        .expect("row");
    assert_eq!(
        row.target_commit.as_deref(),
        Some(sha(2).as_str()),
        "the requested commit wins"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn finished_deployment_visibility_window(pool: PgPool) {
    use crystal_forge::queries::system_events::get_system_deployment_progress_row;
    let fleet = fleet(&pool, true).await;
    let subject = system(&pool, &fleet, "lane-window", fleet.env_a, Some(2)).await;
    let finish = |status: &'static str, age_secs: i32| {
        let pool = pool.clone();
        async move {
            sqlx::query("DELETE FROM pending_system_deployments WHERE system_id=$1")
                .bind(subject)
                .execute(&pool)
                .await
                .expect("clear");
            sqlx::query(
                "INSERT INTO pending_system_deployments(system_id,target_store_path,status,source,issued_at,expires_at,completed_at) \
                 VALUES($1,'/nix/store/lane-window-2',$2,'manual',now() - interval '10 minutes',now() + interval '2 hours', \
                        now() - make_interval(secs => $3))",
            )
            .bind(subject)
            .bind(status)
            .bind(age_secs)
            .execute(&pool)
            .await
            .expect("insert");
            get_system_deployment_progress_row(&pool, subject)
                .await
                .expect("read")
                .is_some()
        }
    };
    assert!(finish("succeeded", 10).await, "fresh success is shown");
    assert!(
        !finish("succeeded", 60).await,
        "success older than 30 s is hidden"
    );
    assert!(finish("failed", 60).await, "failure stays for 2 minutes");
    assert!(!finish("failed", 300).await, "old failure is hidden");
}

#[sqlx::test(migrations = "./migrations")]
async fn running_commit_beyond_the_500_commit_bound_is_reported_not_dropped(pool: PgPool) {
    use crystal_forge::queries::flake_position::FLAKE_POSITION_MAX_COMMITS;
    // 520 branch commits. The host runs the output of commit 510, which is
    // deeper than the bound, so the list stops at the bound and the response
    // says why instead of silently omitting the running commit.
    let flake: i32 = sqlx::query_scalar(
        "INSERT INTO flakes(name,repo_url,branch) VALUES('deep','https://example.invalid/deep','main') RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .expect("flake");
    sqlx::query(
        "INSERT INTO commits(flake_id,git_commit_hash,commit_timestamp,message,author) \
         SELECT $1, lpad(n::text, 40, '0'), now() - make_interval(mins => n), 'c' || n, 'dev' \
         FROM generate_series(0, 519) n",
    )
    .bind(flake)
    .execute(&pool)
    .await
    .expect("commits");
    let env: Uuid =
        sqlx::query_scalar("INSERT INTO environments(name) VALUES('deep-env') RETURNING id")
            .fetch_one(&pool)
            .await
            .expect("environment");
    let subject: Uuid = sqlx::query_scalar(
        "INSERT INTO systems(hostname,public_key,derivation,flake_id,system_configuration_name,environment_id) \
         VALUES('deep-host','test-key','test-derivation',$1,'deep-host',$2) RETURNING id",
    )
    .bind(flake)
    .bind(env)
    .fetch_one(&pool)
    .await
    .expect("system");
    sqlx::query(
        "INSERT INTO derivations(commit_id,derivation_type,derivation_name,derivation_path,status_id,completed_at,store_path) \
         SELECT c.id,'nixos','deep-host','/nix/store/deep-host.drv',10,now(),'/nix/store/deep-host-510' \
         FROM commits c WHERE c.flake_id=$1 AND c.git_commit_hash = lpad('510', 40, '0')",
    )
    .bind(flake)
    .execute(&pool)
    .await
    .expect("derivation");
    sqlx::query("INSERT INTO system_states(hostname,store_path,change_reason) VALUES('deep-host','/nix/store/deep-host-510','startup')")
        .execute(&pool)
        .await
        .expect("state");

    let view = fetch_system_flake_position(&pool, subject, None)
        .await
        .expect("read")
        .expect("system exists");
    assert_eq!(view.commits.len() as i64, FLAKE_POSITION_MAX_COMMITS);
    assert_eq!(view.commits[0].position, 0);
    assert_eq!(
        view.unknown_reason,
        Some(FlakePositionUnknownReason::BeyondWindow)
    );
    assert_eq!(
        view.running_position, None,
        "no position is claimed outside the list"
    );
    assert!(
        view.running_commit.is_some(),
        "the running commit is still named"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn reused_builds_are_classified_through_the_physical_derivation(pool: PgPool) {
    // Commit 3 built and was published. Commit 4 built, but its cache push is
    // pending. Commit 1 reuses commit 3's physical derivation; commit 2 reuses
    // commit 4's physical derivation and also adopts its build job directly.
    // The alias derivations own no build job or cache push of their own.
    let fleet = fleet(&pool, true).await;
    let subject = system(&pool, &fleet, "lane-reuse", fleet.env_a, Some(0)).await;
    let derivation = |commit_index: usize| {
        let pool = pool.clone();
        let commit = fleet.commits[commit_index];
        async move {
            sqlx::query_scalar::<_, i32>(
                "SELECT id FROM derivations WHERE commit_id=$1 AND derivation_name='lane-reuse'",
            )
            .bind(commit)
            .fetch_one(&pool)
            .await
            .expect("derivation")
        }
    };
    let (origin_published, origin_pending) = (derivation(3).await, derivation(4).await);
    let job_of_pending: Uuid = {
        sqlx::query(
            "INSERT INTO build_jobs(derivation_id,status) VALUES($1,'success'),($2,'success')",
        )
        .bind(origin_published)
        .bind(origin_pending)
        .execute(&pool)
        .await
        .expect("jobs");
        sqlx::query_scalar("SELECT id FROM build_jobs WHERE derivation_id=$1")
            .bind(origin_pending)
            .fetch_one(&pool)
            .await
            .expect("job id")
    };
    sqlx::query(
        "INSERT INTO cache_push_jobs(derivation_id,status,store_path) \
         SELECT id,'completed',store_path FROM derivations WHERE id=$1",
    )
    .bind(origin_published)
    .execute(&pool)
    .await
    .expect("published");
    sqlx::query(
        "INSERT INTO cache_push_jobs(derivation_id,status,store_path,error_message) \
         SELECT id,'pending',store_path,'cache_probe_failed' FROM derivations WHERE id=$1",
    )
    .bind(origin_pending)
    .execute(&pool)
    .await
    .expect("pending");

    for (alias_commit, origin, adopted_job) in [
        (1_usize, origin_published, None),
        (2_usize, origin_pending, Some(job_of_pending)),
    ] {
        let alias = derivation(alias_commit).await;
        sqlx::query(
            "UPDATE derivations alias SET derivation_path=origin.derivation_path, \
                    expected_store_path=origin.store_path, store_path=NULL, \
                    reused_from_derivation_id=origin.id, reused_build_job_id=$3 \
             FROM derivations origin WHERE alias.id=$1 AND origin.id=$2",
        )
        .bind(alias)
        .bind(origin)
        .bind(adopted_job)
        .execute(&pool)
        .await
        .expect("alias derivation");
    }

    let view = fetch_system_flake_position(&pool, subject, None)
        .await
        .expect("read")
        .expect("system exists");
    let state = |position: usize| view.commits[position].build_state;
    assert_eq!(
        state(3),
        FlakePositionBuildState::Ready,
        "origin, published"
    );
    assert_eq!(
        state(4),
        FlakePositionBuildState::Unpublished,
        "origin, push pending"
    );
    assert_eq!(
        state(1),
        FlakePositionBuildState::Ready,
        "alias of a published origin"
    );
    assert_eq!(
        state(2),
        FlakePositionBuildState::Unpublished,
        "alias that adopted the job of an unpublished origin"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn latest_observation_decides_not_observed(pool: PgPool) {
    // The host once reported a store path, but its latest report is blank
    // (store_path is NOT NULL, so absence is an empty or whitespace value). The
    // view holds every eligible row, so presence must come from the latest row.
    let fleet = fleet(&pool, true).await;
    let subject = system(&pool, &fleet, "lane-gap", fleet.env_a, None).await;
    sqlx::query(
        "INSERT INTO system_states(hostname,store_path,change_reason,timestamp) VALUES \
         ('lane-gap','/nix/store/lane-gap-1','startup', now() - interval '10 minutes'), \
         ('lane-gap','  ','config_change', now() - interval '1 minute')",
    )
    .execute(&pool)
    .await
    .expect("states");

    let view = fetch_system_flake_position(&pool, subject, None)
        .await
        .expect("read")
        .expect("system exists");
    assert_eq!(
        view.unknown_reason,
        Some(FlakePositionUnknownReason::NotObserved),
        "an older row with a store path must not make the host look observed"
    );
    assert_eq!(view.running_position, None);
}

#[sqlx::test(migrations = "./migrations")]
async fn adopted_job_is_published_on_the_alias_that_executed_it(pool: PgPool) {
    // A (commit 3) is the original physical derivation and never built. B
    // (commit 4) is an execution alias: it owns the successful job and the
    // completed cache push. C (commit 2) is an alias that adopts B's job. The
    // publication evidence belongs to B, not to the origin A.
    let fleet = fleet(&pool, true).await;
    let subject = system(&pool, &fleet, "lane-alias", fleet.env_a, Some(0)).await;
    let derivation = |commit_index: usize| {
        let pool = pool.clone();
        let commit = fleet.commits[commit_index];
        async move {
            sqlx::query_scalar::<_, i32>(
                "SELECT id FROM derivations WHERE commit_id=$1 AND derivation_name='lane-alias'",
            )
            .bind(commit)
            .fetch_one(&pool)
            .await
            .expect("derivation")
        }
    };
    let (a, b, c) = (
        derivation(3).await,
        derivation(4).await,
        derivation(2).await,
    );

    // B becomes an alias of A first: a derivation's identity is locked once it
    // has execution evidence.
    sqlx::query(
        "UPDATE derivations alias SET derivation_path=origin.derivation_path, \
                expected_store_path=origin.store_path, store_path=origin.store_path, \
                reused_from_derivation_id=origin.id \
         FROM derivations origin WHERE alias.id=$1 AND origin.id=$2",
    )
    .bind(b)
    .bind(a)
    .execute(&pool)
    .await
    .expect("execution alias");
    let job_of_b: Uuid = sqlx::query_scalar(
        "INSERT INTO build_jobs(derivation_id,status) VALUES($1,'success') RETURNING id",
    )
    .bind(b)
    .fetch_one(&pool)
    .await
    .expect("job on the execution alias");
    sqlx::query(
        "INSERT INTO cache_push_jobs(derivation_id,status,store_path) \
         SELECT id,'completed',store_path FROM derivations WHERE id=$1",
    )
    .bind(b)
    .execute(&pool)
    .await
    .expect("push on the execution alias");
    sqlx::query(
        "UPDATE derivations alias SET derivation_path=origin.derivation_path, \
                expected_store_path=origin.store_path, store_path=NULL, \
                reused_from_derivation_id=origin.id, reused_build_job_id=$3 \
         FROM derivations origin WHERE alias.id=$1 AND origin.id=$2",
    )
    .bind(c)
    .bind(a)
    .bind(job_of_b)
    .execute(&pool)
    .await
    .expect("adopting alias");

    let view = fetch_system_flake_position(&pool, subject, None)
        .await
        .expect("read")
        .expect("system exists");
    let state = |position: usize| view.commits[position].build_state;
    assert_eq!(
        state(4),
        FlakePositionBuildState::Ready,
        "the execution alias built and published"
    );
    assert_eq!(
        state(2),
        FlakePositionBuildState::Ready,
        "the adopting alias is published through the alias that executed the job"
    );
    assert_eq!(
        state(3),
        FlakePositionBuildState::NotBuilt,
        "the origin evaluator owns no build"
    );
}
