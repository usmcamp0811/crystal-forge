//! Enforce-mode unresolved-policy validation for the authoritative resolver.
//!
//! TASK-412 requires:
//! - enforce-mode assignments reject effective `unbound`/`opaque` policies;
//! - an unresolved baseline policy that is excluded before effective-set
//!   construction does not block enforcement;
//! - report-only assignments accept unresolved policies;
//! - the combined environment/system resolution path does not silently bypass
//!   the rule.
//!
//! These tests exercise the real resolver against a disposable PostgreSQL
//! database (see `CRYSTAL_FORGE_TEST_DATABASE_URL`).

use crystal_forge::compliance::resolver::{
    AssignmentMode, AssignmentTarget, EffectivePolicyResolutionInput, PolicySpecificity,
    ResolutionOutcome, resolve_effective_policy_set, resolve_system_effective_policies,
};
use sqlx::PgPool;
use uuid::Uuid;

async fn pool() -> PgPool {
    PgPool::connect(
        &std::env::var("CRYSTAL_FORGE_TEST_DATABASE_URL")
            .expect("CRYSTAL_FORGE_TEST_DATABASE_URL must name the disposable test database"),
    )
    .await
    .expect("connect to test database")
}

/// Create a policy whose current draft version has the given implementation
/// state. Returns (policy lineage id, policy version id).
async fn policy_with_implementation_state(pool: &PgPool, state: &str) -> (Uuid, Uuid) {
    let policy_id: Uuid = sqlx::query_scalar(
        "INSERT INTO deployment_policies (name, policy_type, config, enabled) VALUES ($1, 'custom_check', '{\"expression\":\"true\"}', true) RETURNING id",
    )
    .bind(format!("resolver-enforcement-{}", Uuid::new_v4()))
    .fetch_one(pool)
    .await
    .expect("insert policy");
    let version_id: Uuid = sqlx::query_scalar(
        "SELECT current_draft_version_id FROM deployment_policies WHERE id = $1",
    )
    .bind(policy_id)
    .fetch_one(pool)
    .await
    .expect("policy draft version");
    sqlx::query("UPDATE deployment_policy_versions SET implementation_state = $1 WHERE id = $2")
        .bind(state)
        .bind(version_id)
        .execute(pool)
        .await
        .expect("set implementation state");
    (policy_id, version_id)
}

/// Publish a policy version: accepted, trusted, and the lineage's published
/// pointer.
async fn accept_policy(pool: &PgPool, policy_id: Uuid, version_id: Uuid) {
    let mut tx = pool.begin().await.expect("begin policy publication");
    sqlx::query("UPDATE deployment_policies SET current_draft_version_id = NULL WHERE id = $1")
        .bind(policy_id)
        .execute(&mut *tx)
        .await
        .expect("clear policy draft pointer");
    sqlx::query(
        "UPDATE deployment_policy_versions
         SET publication_state = 'accepted', published_at = CURRENT_TIMESTAMP,
             trust_state = 'trusted'
         WHERE id = $1",
    )
    .bind(version_id)
    .execute(&mut *tx)
    .await
    .expect("accept policy version");
    sqlx::query("UPDATE deployment_policies SET current_published_version_id = $1 WHERE id = $2")
        .bind(version_id)
        .bind(policy_id)
        .execute(&mut *tx)
        .await
        .expect("set policy published pointer");
    tx.commit().await.expect("commit policy publication");
}

/// Create a draft bundle. Returns (bundle lineage id, bundle version id).
async fn draft_bundle(pool: &PgPool) -> (Uuid, Uuid) {
    let bundle_id: Uuid = sqlx::query_scalar(
        "INSERT INTO compliance_bundles (name, framework, version, layer) VALUES ($1, 'test', '1.0', 'fleet') RETURNING id",
    )
    .bind(format!("resolver-enforcement-{}", Uuid::new_v4()))
    .fetch_one(pool)
    .await
    .expect("insert bundle");
    let version_id: Uuid =
        sqlx::query_scalar("SELECT current_draft_version_id FROM compliance_bundles WHERE id = $1")
            .bind(bundle_id)
            .fetch_one(pool)
            .await
            .expect("bundle draft version");
    (bundle_id, version_id)
}

/// Add one policy version as the ordered baseline member, then fix the bundle
/// digest (membership changes invalidate it to 'pending').
async fn add_member(pool: &PgPool, bundle_version_id: Uuid, policy_version_id: Uuid) {
    sqlx::query(
        "INSERT INTO compliance_bundle_version_policies (bundle_version_id, policy_version_id, policy_order) VALUES ($1, $2, 0)",
    )
    .bind(bundle_version_id)
    .bind(policy_version_id)
    .execute(pool)
    .await
    .expect("add bundle member");
    sqlx::query(
        "UPDATE compliance_bundle_versions SET semantic_digest = 'test-digest' WHERE id = $1",
    )
    .bind(bundle_version_id)
    .execute(pool)
    .await
    .expect("fix bundle digest");
}

/// Publish a bundle version: accepted and trusted.
async fn accept_bundle(pool: &PgPool, bundle_id: Uuid, version_id: Uuid) {
    let mut tx = pool.begin().await.expect("begin bundle publication");
    sqlx::query("UPDATE compliance_bundles SET current_draft_version_id = NULL WHERE id = $1")
        .bind(bundle_id)
        .execute(&mut *tx)
        .await
        .expect("clear bundle draft pointer");
    sqlx::query(
        "UPDATE compliance_bundle_versions
         SET publication_state = 'accepted', published_at = CURRENT_TIMESTAMP,
             trust_state = 'trusted', semantic_digest = 'test-digest'
         WHERE id = $1",
    )
    .bind(version_id)
    .execute(&mut *tx)
    .await
    .expect("accept bundle version");
    sqlx::query("UPDATE compliance_bundles SET current_published_version_id = $1 WHERE id = $2")
        .bind(version_id)
        .bind(bundle_id)
        .execute(&mut *tx)
        .await
        .expect("set bundle published pointer");
    tx.commit().await.expect("commit bundle publication");
}

async fn resolve(
    pool: &PgPool,
    bundle_version_id: Uuid,
    exclusions: Vec<Uuid>,
    additions: Vec<Uuid>,
    mode: AssignmentMode,
) -> ResolutionOutcome {
    let mut tx = pool.begin().await.expect("begin resolution transaction");
    let input = EffectivePolicyResolutionInput {
        target: AssignmentTarget::Environment {
            environment_id: Uuid::new_v4(),
        },
        bundle_version_id,
        exclusions,
        additions,
        overrides: Vec::new(),
        assignment_mode: mode,
        specificity: PolicySpecificity::BundleBaseline,
    };
    resolve_effective_policy_set(&mut tx, &input)
        .await
        .expect("resolve effective policy set")
}

fn assert_unresolved_conflict(outcome: &ResolutionOutcome) {
    match outcome {
        ResolutionOutcome::Conflict(conflicts) => {
            assert!(
                conflicts
                    .iter()
                    .any(|c| c.code == "UNRESOLVED_ENFORCEMENT_POLICY"),
                "expected UNRESOLVED_ENFORCEMENT_POLICY conflict, got {conflicts:?}"
            );
        }
        ResolutionOutcome::Resolved(_) => panic!("expected an unresolved-policy conflict"),
    }
}

// ── Tests A–G: single-assignment preview ──────────────────────────────────────

#[tokio::test]
async fn enforce_baseline_unbound_rejected() {
    let pool = pool().await;
    let (policy_id, version_id) = policy_with_implementation_state(&pool, "unbound").await;
    accept_policy(&pool, policy_id, version_id).await;
    let (bundle_id, bundle_version_id) = draft_bundle(&pool).await;
    add_member(&pool, bundle_version_id, version_id).await;
    accept_bundle(&pool, bundle_id, bundle_version_id).await;

    let outcome = resolve(
        &pool,
        bundle_version_id,
        vec![],
        vec![],
        AssignmentMode::Enforce,
    )
    .await;
    assert_unresolved_conflict(&outcome);
}

#[tokio::test]
async fn enforce_baseline_opaque_rejected() {
    let pool = pool().await;
    let (policy_id, version_id) = policy_with_implementation_state(&pool, "opaque").await;
    accept_policy(&pool, policy_id, version_id).await;
    let (bundle_id, bundle_version_id) = draft_bundle(&pool).await;
    add_member(&pool, bundle_version_id, version_id).await;
    accept_bundle(&pool, bundle_id, bundle_version_id).await;

    let outcome = resolve(
        &pool,
        bundle_version_id,
        vec![],
        vec![],
        AssignmentMode::Enforce,
    )
    .await;
    assert_unresolved_conflict(&outcome);
}

#[tokio::test]
async fn enforce_baseline_unbound_excluded_succeeds() {
    let pool = pool().await;
    let (policy_id, version_id) = policy_with_implementation_state(&pool, "unbound").await;
    accept_policy(&pool, policy_id, version_id).await;
    let (bundle_id, bundle_version_id) = draft_bundle(&pool).await;
    add_member(&pool, bundle_version_id, version_id).await;
    accept_bundle(&pool, bundle_id, bundle_version_id).await;

    let outcome = resolve(
        &pool,
        bundle_version_id,
        vec![version_id],
        vec![],
        AssignmentMode::Enforce,
    )
    .await;
    match outcome {
        ResolutionOutcome::Resolved(set) => {
            assert!(
                set.policies
                    .iter()
                    .all(|p| p.policy_version_id != version_id),
                "excluded policy must not be in the effective set"
            );
        }
        ResolutionOutcome::Conflict(conflicts) => {
            panic!("excluded unresolved policy must not block enforcement: {conflicts:?}")
        }
    }
}

#[tokio::test]
async fn enforce_unbound_addition_rejected() {
    let pool = pool().await;
    let (policy_id, version_id) = policy_with_implementation_state(&pool, "unbound").await;
    accept_policy(&pool, policy_id, version_id).await;
    let (bundle_id, bundle_version_id) = draft_bundle(&pool).await;
    accept_bundle(&pool, bundle_id, bundle_version_id).await;

    let outcome = resolve(
        &pool,
        bundle_version_id,
        vec![],
        vec![version_id],
        AssignmentMode::Enforce,
    )
    .await;
    assert_unresolved_conflict(&outcome);
}

#[tokio::test]
async fn enforce_opaque_addition_rejected() {
    let pool = pool().await;
    let (policy_id, version_id) = policy_with_implementation_state(&pool, "opaque").await;
    accept_policy(&pool, policy_id, version_id).await;
    let (bundle_id, bundle_version_id) = draft_bundle(&pool).await;
    accept_bundle(&pool, bundle_id, bundle_version_id).await;

    let outcome = resolve(
        &pool,
        bundle_version_id,
        vec![],
        vec![version_id],
        AssignmentMode::Enforce,
    )
    .await;
    assert_unresolved_conflict(&outcome);
}

#[tokio::test]
async fn report_only_unbound_succeeds() {
    let pool = pool().await;
    let (policy_id, version_id) = policy_with_implementation_state(&pool, "unbound").await;
    accept_policy(&pool, policy_id, version_id).await;
    let (bundle_id, bundle_version_id) = draft_bundle(&pool).await;
    add_member(&pool, bundle_version_id, version_id).await;
    accept_bundle(&pool, bundle_id, bundle_version_id).await;

    let outcome = resolve(
        &pool,
        bundle_version_id,
        vec![],
        vec![],
        AssignmentMode::ReportOnly,
    )
    .await;
    match outcome {
        ResolutionOutcome::Resolved(set) => {
            assert_eq!(set.policies.len(), 1);
        }
        ResolutionOutcome::Conflict(conflicts) => {
            panic!("report-only must accept unresolved policies: {conflicts:?}")
        }
    }
}

#[tokio::test]
async fn report_only_opaque_succeeds() {
    let pool = pool().await;
    let (policy_id, version_id) = policy_with_implementation_state(&pool, "opaque").await;
    accept_policy(&pool, policy_id, version_id).await;
    let (bundle_id, bundle_version_id) = draft_bundle(&pool).await;
    add_member(&pool, bundle_version_id, version_id).await;
    accept_bundle(&pool, bundle_id, bundle_version_id).await;

    let outcome = resolve(
        &pool,
        bundle_version_id,
        vec![],
        vec![],
        AssignmentMode::ReportOnly,
    )
    .await;
    match outcome {
        ResolutionOutcome::Resolved(set) => {
            assert_eq!(set.policies.len(), 1);
        }
        ResolutionOutcome::Conflict(conflicts) => {
            panic!("report-only must accept unresolved policies: {conflicts:?}")
        }
    }
}

// ── Test H: combined environment/system resolution ────────────────────────────

#[tokio::test]
async fn combined_resolution_does_not_bypass_enforce_validation() {
    let pool = pool().await;

    // Environment + system inside it.
    let environment_id: Uuid =
        sqlx::query_scalar("INSERT INTO environments (name) VALUES ($1) RETURNING id")
            .bind(format!("resolver-env-{}", Uuid::new_v4()))
            .fetch_one(&pool)
            .await
            .expect("insert environment");
    let system_id: Uuid = sqlx::query_scalar(
        "INSERT INTO systems (hostname, environment_id, public_key, derivation) VALUES ($1, $2, 'test-key', '/nix/store/test') RETURNING id",
    )
    .bind(format!("resolver-system-{}", Uuid::new_v4()))
    .bind(environment_id)
    .fetch_one(&pool)
    .await
    .expect("insert system");

    // Bundle with one unbound baseline member, published and trusted.
    let (policy_id, version_id) = policy_with_implementation_state(&pool, "unbound").await;
    accept_policy(&pool, policy_id, version_id).await;
    let (bundle_id, bundle_version_id) = draft_bundle(&pool).await;
    add_member(&pool, bundle_version_id, version_id).await;
    accept_bundle(&pool, bundle_id, bundle_version_id).await;

    // Enforce-mode environment assignment backed by an immutable version row.
    let assignment_id: Uuid = sqlx::query_scalar(
        r#"INSERT INTO compliance_bundle_assignments
             (bundle_version_id, scope_type, environment_id, enforcement_mode,
              assignment_overlay_digest, bundle_id)
           VALUES ($1, 'environment', $2, 'enforce', 'digest', $3)
           RETURNING id"#,
    )
    .bind(bundle_version_id)
    .bind(environment_id)
    .bind(bundle_id)
    .fetch_one(&pool)
    .await
    .expect("insert assignment");
    let assignment_version_id: Uuid = sqlx::query_scalar(
        r#"INSERT INTO compliance_bundle_assignment_versions
             (assignment_id, version_number, bundle_version_id, enforcement_mode,
              assignment_overlay_digest)
           VALUES ($1, 1, $2, 'enforce', 'digest')
           RETURNING id"#,
    )
    .bind(assignment_id)
    .bind(bundle_version_id)
    .fetch_one(&pool)
    .await
    .expect("insert assignment version");
    sqlx::query("UPDATE compliance_bundle_assignments SET current_version_id = $1 WHERE id = $2")
        .bind(assignment_version_id)
        .bind(assignment_id)
        .execute(&pool)
        .await
        .expect("set assignment current version");

    let outcome = resolve_system_effective_policies(&pool, system_id)
        .await
        .expect("resolve system effective policies");
    assert_unresolved_conflict(&outcome);
}
