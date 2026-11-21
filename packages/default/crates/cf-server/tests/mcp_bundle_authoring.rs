//! MCP bundle draft and impact preview regressions (TASK-534.8).
//!
//! Each SQLx test owns an isolated database with the full production
//! migrations. Tokens are issued through the access-token service, so the
//! in-transaction token checks see real token rows. The direct tests call
//! `call_tool` with a caller built from the issued token, the way the
//! transport builds it at admission; this lets a test change the owner or
//! the grant between admission and the write. `full_agent_workflow_over_http`
//! speaks JSON-RPC to the real `/mcp` handler.
//!
//! Run with:
//! `cargo test -p cf-server --test mcp_bundle_authoring -- --test-threads=1`

use std::sync::Arc;
use std::time::Duration as StdDuration;

use axum::{Router, extract::DefaultBodyLimit, routing::post};
use chrono::{Duration, Utc};
use crystal_forge::api::models::{CreateAccessTokenRequest, UpdateComplianceBundleRequest};
use crystal_forge::auth::authorization::RestrictedGrant;
use crystal_forge::handlers::agent_request::CFState;
use crystal_forge::mcp;
use crystal_forge::mcp::tools::{ToolCaller, ToolError, call_tool};
use crystal_forge::models::auth_identity::AuthRole;
use crystal_forge::queries::auth_identity::{insert_user_environment_membership, sync_user_role};
use crystal_forge::queries::compliance::{PolicyDraftIntent, ensure_policy_draft, update_bundle};
use crystal_forge::queries::users::insert_user;
use crystal_forge::services::access_tokens::{self as token_service, AccessTokenSettings};
use crystal_forge::services::bundle_authoring::{
    BundleDraftChanges, NewBundleLineage, create_bundle_lineage_draft, derive_bundle_draft,
    update_bundle_draft,
};
use crystal_forge::services::policy_authoring::{AuthoringActor, source_sha256};
use crystal_forge::{
    config::ServerConfig, queue::QueueNotifier, server::jobs::BackgroundJobRegistry,
};
use reqwest::Client;
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

const BUNDLES: &[&str] = &["bundles:read", "bundles:draft"];
const PREVIEW: &[&str] = &["compliance:preview"];

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

fn settings() -> AccessTokenSettings {
    AccessTokenSettings {
        enabled: true,
        default_lifetime_days: 30,
        max_lifetime_days: 90,
        entitlement_max_age_seconds: 86_400,
        mcp_enabled: true,
        mcp_public_url: None,
    }
}

fn unique(prefix: &str) -> String {
    format!("{prefix}-{}", &Uuid::new_v4().simple().to_string()[..10])
}

async fn environment(pool: &PgPool) -> Uuid {
    sqlx::query_scalar("INSERT INTO environments(name) VALUES($1) RETURNING id")
        .bind(unique("bundles"))
        .fetch_one(pool)
        .await
        .expect("insert environment")
}

async fn user(pool: &PgPool, role: AuthRole) -> Uuid {
    let suffix = Uuid::new_v4().simple().to_string();
    let user = insert_user(
        pool,
        &format!("bundles-{suffix}@example.invalid"),
        Some("Bundle Authoring Test"),
    )
    .await
    .expect("insert user");
    sync_user_role(pool, user.id, role)
        .await
        .expect("assign role");
    user.id
}

/// One issued token with its plaintext bearer secret.
struct Token {
    id: Uuid,
    owner: Uuid,
    name: String,
    secret: String,
    environment: Uuid,
}

fn token_request(name: &str, environment: Uuid, scopes: &[&str]) -> CreateAccessTokenRequest {
    serde_json::from_value(json!({
        "name": name,
        "audience": "mcp",
        "expires_in_days": 30,
        "environment_ids": [environment.to_string()],
        "scopes": scopes,
    }))
    .expect("token request")
}

async fn issue(pool: &PgPool, owner: Uuid, scopes: &[&str]) -> Token {
    let environment = environment(pool).await;
    insert_user_environment_membership(pool, owner, environment)
        .await
        .expect("assign membership");
    let name = unique("agent");
    let issued = token_service::create(
        pool,
        owner,
        &token_request(&name, environment, scopes),
        &settings(),
    )
    .await
    .expect("issue token");
    Token {
        id: issued.metadata.id,
        owner,
        name,
        secret: issued.secret.expose().to_owned(),
        environment,
    }
}

/// Builds the caller that the transport builds at admission. `scopes` is the
/// admitted grant; it may differ from the token row to prove that the write
/// checks the row.
fn caller(token: &Token, scopes: &[&str], owner_is_admin: bool) -> ToolCaller {
    ToolCaller {
        token_id: token.id,
        owner_user_id: token.owner,
        owner_is_admin,
        owner_environment_ids: vec![token.environment],
        grant: RestrictedGrant::new(
            token.id,
            Some("mcp".into()),
            scopes.iter().map(|scope| (*scope).to_owned()).collect(),
            [token.environment].into(),
            Utc::now() + Duration::hours(1),
        ),
    }
}

async fn admin_author(pool: &PgPool, scopes: &[&str]) -> (Token, ToolCaller) {
    let owner = user(pool, AuthRole::Admin).await;
    let token = issue(pool, owner, scopes).await;
    let caller = caller(&token, scopes, true);
    (token, caller)
}

async fn call(caller: &ToolCaller, pool: &PgPool, name: &str, arguments: Value) -> Value {
    call_tool(pool, caller, name, arguments)
        .await
        .unwrap_or_else(|error| panic!("{name} failed: {error:?}"))
}

async fn call_err(caller: &ToolCaller, pool: &PgPool, name: &str, arguments: Value) -> ToolError {
    match call_tool(pool, caller, name, arguments).await {
        Ok(value) => panic!("{name} unexpectedly succeeded: {value}"),
        Err(error) => error,
    }
}

fn invalid_message(error: ToolError) -> String {
    match error {
        ToolError::InvalidArguments(message) => message,
        other => panic!("expected invalid_arguments, got {other:?}"),
    }
}

fn uuid(value: &Value) -> Uuid {
    serde_json::from_value(value.clone()).expect("uuid value")
}

/// Row counts of every table that a bundle authoring write changes.
async fn counts(pool: &PgPool) -> (i64, i64, i64, i64, i64) {
    sqlx::query_as(
        "SELECT (SELECT COUNT(*) FROM compliance_bundles), \
                (SELECT COUNT(*) FROM compliance_bundle_versions), \
                (SELECT COUNT(*) FROM compliance_bundle_version_policies), \
                (SELECT COUNT(*) FROM compliance_bundle_policies), \
                (SELECT COUNT(*) FROM admin_audit_events)",
    )
    .fetch_one(pool)
    .await
    .expect("count rows")
}

/// Counts and states that an impact preview must never change: assignments,
/// assignment versions, scheduled work, and publication and trust states.
async fn preview_side_effects(pool: &PgPool) -> (i64, i64, i64, i64, i64, String, String) {
    sqlx::query_as(
        "SELECT (SELECT COUNT(*) FROM compliance_bundle_assignments), \
                (SELECT COUNT(*) FROM compliance_bundle_assignment_versions), \
                (SELECT COUNT(*) FROM cve_scans), \
                (SELECT COUNT(*) FROM pending_system_deployments), \
                (SELECT COUNT(*) FROM build_jobs), \
                (SELECT COALESCE(string_agg(id::text || publication_state || trust_state \
                    || draft_revision::text || semantic_digest, ',' ORDER BY id), '') \
                   FROM compliance_bundle_versions), \
                (SELECT COALESCE(string_agg(id::text || publication_state || trust_state, ',' \
                    ORDER BY id), '') FROM deployment_policy_versions)",
    )
    .fetch_one(pool)
    .await
    .expect("preview side effects")
}

async fn bundle_revision(pool: &PgPool, version_id: Uuid) -> i64 {
    sqlx::query_scalar("SELECT draft_revision FROM compliance_bundle_versions WHERE id = $1")
        .bind(version_id)
        .fetch_one(pool)
        .await
        .expect("bundle draft revision")
}

/// Returns the exact ordered membership of a bundle version.
async fn members(pool: &PgPool, version_id: Uuid) -> Vec<Uuid> {
    sqlx::query_scalar(
        "SELECT policy_version_id FROM compliance_bundle_version_policies \
         WHERE bundle_version_id = $1 ORDER BY policy_order",
    )
    .bind(version_id)
    .fetch_all(pool)
    .await
    .expect("bundle members")
}

/// Returns the sorted lineage-level membership of a bundle.
async fn lineages(pool: &PgPool, bundle_id: Uuid) -> Vec<Uuid> {
    sqlx::query_scalar(
        "SELECT policy_id FROM compliance_bundle_policies WHERE bundle_id = $1 ORDER BY policy_id",
    )
    .bind(bundle_id)
    .fetch_all(pool)
    .await
    .expect("bundle lineages")
}

fn sorted(mut ids: Vec<Uuid>) -> Vec<Uuid> {
    ids.sort();
    ids
}

/// Returns the `(action, access_token_id, metadata)` audit rows of a target.
async fn audit_rows(pool: &PgPool, target: Uuid) -> Vec<(String, Option<Uuid>, Value)> {
    sqlx::query_as(
        "SELECT action, access_token_id, metadata FROM admin_audit_events \
         WHERE target = $1 ORDER BY created_at, id",
    )
    .bind(target.to_string())
    .fetch_all(pool)
    .await
    .expect("audit rows")
}

/// Inserts a policy lineage; the lineage trigger creates its untrusted draft.
/// Returns `(lineage_id, draft_version_id)`.
async fn draft_policy(pool: &PgPool, name: &str) -> (Uuid, Uuid) {
    let policy_id: Uuid = sqlx::query_scalar(
        "INSERT INTO deployment_policies (name, description, policy_type, config, enabled) \
         VALUES ($1, 'Bundle fixture', 'custom_check', $2, false) RETURNING id",
    )
    .bind(name)
    .bind(json!({"mode": "all", "rules": []}))
    .fetch_one(pool)
    .await
    .expect("insert policy lineage");
    let version_id: Uuid = sqlx::query_scalar(
        "SELECT current_draft_version_id FROM deployment_policies WHERE id = $1",
    )
    .bind(policy_id)
    .fetch_one(pool)
    .await
    .expect("draft version");
    (policy_id, version_id)
}

/// Inserts a policy lineage and publishes its version as trusted, the way
/// the production publish path leaves it.
async fn published_policy(pool: &PgPool, name: &str) -> (Uuid, Uuid) {
    let (policy_id, version_id) = draft_policy(pool, name).await;
    let mut tx = pool.begin().await.expect("begin publish");
    sqlx::query("UPDATE deployment_policies SET current_draft_version_id = NULL WHERE id = $1")
        .bind(policy_id)
        .execute(&mut *tx)
        .await
        .expect("clear draft pointer");
    sqlx::query(
        "UPDATE deployment_policy_versions SET publication_state = 'accepted', \
         trust_state = 'trusted', trusted_at = NOW(), published_at = NOW() WHERE id = $1",
    )
    .bind(version_id)
    .execute(&mut *tx)
    .await
    .expect("accept version");
    sqlx::query("UPDATE deployment_policies SET current_published_version_id = $1 WHERE id = $2")
        .bind(version_id)
        .bind(policy_id)
        .execute(&mut *tx)
        .await
        .expect("set published pointer");
    tx.commit().await.expect("commit publish");
    (policy_id, version_id)
}

/// Derives a new untrusted draft of a published policy lineage.
async fn newer_policy_draft(pool: &PgPool, policy_id: Uuid) -> Uuid {
    let mut tx = pool.begin().await.expect("begin derive");
    let version_id = ensure_policy_draft(
        &mut tx,
        policy_id,
        None,
        None,
        PolicyDraftIntent::CreateExplicit,
    )
    .await
    .expect("derive policy draft");
    tx.commit().await.expect("commit derive");
    version_id
}

/// Creates a bundle lineage with the given policy lineages and publishes its
/// version as trusted with a computed digest. Returns
/// `(bundle_id, version_id)`.
async fn published_bundle(pool: &PgPool, policy_ids: &[Uuid]) -> (Uuid, Uuid) {
    let bundle_id: Uuid = sqlx::query_scalar(
        "INSERT INTO compliance_bundles (name, framework, version, layer, owner) \
         VALUES ($1, 'CF-TEST', '1.0', 'fleet', 'Tests') RETURNING id",
    )
    .bind(unique("published-bundle"))
    .fetch_one(pool)
    .await
    .expect("insert bundle");
    for policy_id in policy_ids {
        // The lineage membership trigger adds the lineage's published version.
        sqlx::query(
            "INSERT INTO compliance_bundle_policies (bundle_id, policy_id) VALUES ($1, $2)",
        )
        .bind(bundle_id)
        .bind(policy_id)
        .execute(pool)
        .await
        .expect("insert bundle lineage member");
    }
    let version_id: Uuid =
        sqlx::query_scalar("SELECT current_draft_version_id FROM compliance_bundles WHERE id = $1")
            .bind(bundle_id)
            .fetch_one(pool)
            .await
            .expect("bundle draft");
    let mut tx = pool.begin().await.expect("begin bundle publish");
    sqlx::query("UPDATE compliance_bundles SET current_draft_version_id = NULL WHERE id = $1")
        .bind(bundle_id)
        .execute(&mut *tx)
        .await
        .expect("clear bundle draft pointer");
    sqlx::query(
        "UPDATE compliance_bundle_versions SET publication_state = 'accepted', \
         trust_state = 'trusted', semantic_digest = $2, published_at = NOW() WHERE id = $1",
    )
    .bind(version_id)
    .bind(format!("fixture-digest-{version_id}"))
    .execute(&mut *tx)
    .await
    .expect("accept bundle version");
    sqlx::query("UPDATE compliance_bundles SET current_published_version_id = $1 WHERE id = $2")
        .bind(version_id)
        .bind(bundle_id)
        .execute(&mut *tx)
        .await
        .expect("set bundle published pointer");
    tx.commit().await.expect("commit bundle publish");
    (bundle_id, version_id)
}

/// Assigns a published bundle version to an environment in enforce mode.
async fn assign_environment(
    pool: &PgPool,
    bundle_id: Uuid,
    bundle_version_id: Uuid,
    environment_id: Uuid,
) {
    let assignment_id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO compliance_bundle_assignments
           (id, bundle_id, bundle_version_id, environment_id, scope_type, active,
            assignment_overlay_digest, created_at, updated_at)
           VALUES ($1, $2, $3, $4, 'environment', true, 'test-digest',
                   CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)"#,
    )
    .bind(assignment_id)
    .bind(bundle_id)
    .bind(bundle_version_id)
    .bind(environment_id)
    .execute(pool)
    .await
    .expect("create environment assignment");
    let version_id: Uuid = sqlx::query_scalar(
        r#"INSERT INTO compliance_bundle_assignment_versions
           (assignment_id, version_number, bundle_version_id, enforcement_mode,
            assignment_overlay_digest, created_at)
           VALUES ($1, 1, $2, 'enforce', 'test-digest', CURRENT_TIMESTAMP)
           RETURNING id"#,
    )
    .bind(assignment_id)
    .bind(bundle_version_id)
    .fetch_one(pool)
    .await
    .expect("create assignment version");
    sqlx::query("UPDATE compliance_bundle_assignments SET current_version_id = $1 WHERE id = $2")
        .bind(version_id)
        .bind(assignment_id)
        .execute(pool)
        .await
        .expect("set current assignment version");
}

async fn system(pool: &PgPool, hostname: &str, environment_id: Uuid) -> Uuid {
    let system_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO systems (id, hostname, environment_id, public_key, derivation) \
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(system_id)
    .bind(hostname)
    .bind(environment_id)
    .bind(format!("ssh-ed25519 {hostname}"))
    .bind("/nix/store/test-system")
    .execute(pool)
    .await
    .expect("insert system");
    system_id
}

fn new_bundle(name: &str, members: &[Uuid]) -> Value {
    json!({
        "mode": "new_lineage",
        "name": name,
        "description": "Agent bundle",
        "framework": "NIST 800-53",
        "framework_version": "rev5",
        "policy_version_ids": members,
    })
}

fn warning_codes(result: &Value) -> Vec<(String, Uuid)> {
    result["warnings"]
        .as_array()
        .expect("warnings")
        .iter()
        .map(|warning| {
            (
                warning["code"].as_str().expect("code").to_owned(),
                uuid(&warning["policy_version_id"]),
            )
        })
        .collect()
}

/// Returns the `(lineage, version)` pairs of a preview policy list.
fn pairs(policies: &Value) -> Vec<(Uuid, Uuid)> {
    policies
        .as_array()
        .expect("policy list")
        .iter()
        .map(|policy| {
            (
                uuid(&policy["policy_lineage_id"]),
                uuid(&policy["policy_version_id"]),
            )
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Drafts
// ---------------------------------------------------------------------------

#[sqlx::test]
async fn admin_token_creates_new_and_derived_bundle_drafts_with_exact_membership(pool: PgPool) {
    let (token, author) = admin_author(&pool, BUNDLES).await;
    let (published_lineage, published_version) =
        published_policy(&pool, &unique("published")).await;
    // The lineage now also has a newer draft. The lineage trigger would
    // select it; exact membership must keep the requested published version.
    let newer = newer_policy_draft(&pool, published_lineage).await;
    let (draft_lineage, draft_version) = draft_policy(&pool, &unique("draft")).await;

    let created = call(
        &author,
        &pool,
        "create_bundle_draft",
        new_bundle(&unique("agent-bundle"), &[draft_version, published_version]),
    )
    .await;
    let bundle_id = uuid(&created["bundle_id"]);
    let version_id = uuid(&created["version_id"]);
    assert_eq!(created["mode"], "new_lineage");
    assert_eq!(
        created["draft_revision"],
        bundle_revision(&pool, version_id).await
    );
    assert_eq!(
        members(&pool, version_id).await,
        vec![draft_version, published_version]
    );
    assert!(!members(&pool, version_id).await.contains(&newer));
    assert_eq!(
        lineages(&pool, bundle_id).await,
        sorted(vec![draft_lineage, published_lineage])
    );
    assert_eq!(
        warning_codes(&created),
        vec![
            ("draft_member".to_owned(), draft_version),
            ("untrusted_member".to_owned(), draft_version),
        ]
    );
    let row: (
        Option<Uuid>,
        bool,
        Option<Uuid>,
        Option<String>,
        String,
        String,
        String,
        Option<String>,
    ) = sqlx::query_as(
        "SELECT created_by, created_via_agent, created_access_token_id, \
                created_access_token_name, publication_state, trust_state, semantic_digest, \
                framework_version \
         FROM compliance_bundle_versions WHERE id = $1",
    )
    .bind(version_id)
    .fetch_one(&pool)
    .await
    .expect("bundle version row");
    assert_eq!(row.0, Some(token.owner));
    assert!(row.1);
    assert_eq!(row.2, Some(token.id));
    assert_eq!(row.3.as_deref(), Some(token.name.as_str()));
    assert_eq!((row.4.as_str(), row.5.as_str()), ("draft", "untrusted"));
    assert_ne!(row.6, "pending", "the digest is computed in the write");
    assert_eq!(row.7.as_deref(), Some("rev5"));
    let audits = audit_rows(&pool, version_id).await;
    assert_eq!(audits.len(), 1);
    assert_eq!(audits[0].0, "bundle_draft_created");
    assert_eq!(audits[0].1, Some(token.id));
    assert_eq!(audits[0].2["via"], "mcp");
    assert_eq!(audits[0].2["operation"], "new_lineage");
    assert_eq!(audits[0].2["access_token_name"], token.name);
    assert_eq!(audits[0].2["member_count"], 2);

    // The read tool reports the revision and the agent attribution.
    let read = call(
        &author,
        &pool,
        "get_compliance_bundle",
        json!({"version_id": version_id}),
    )
    .await;
    assert_eq!(read["version"]["draft_revision"], created["draft_revision"]);
    assert_eq!(read["version"]["provenance"]["created_via_agent"], true);
    assert_eq!(read["authoring"]["can_edit_draft"], true);

    // A derived draft keeps the published version's exact membership.
    let (other_lineage, _) = published_policy(&pool, &unique("other")).await;
    let (published_bundle_id, published_bundle_version) =
        published_bundle(&pool, &[other_lineage]).await;
    let derived = call(
        &author,
        &pool,
        "create_bundle_draft",
        json!({"mode": "derive", "bundle_id": published_bundle_id}),
    )
    .await;
    let derived_id = uuid(&derived["version_id"]);
    assert_eq!(
        derived["derived_from_version_id"],
        published_bundle_version.to_string()
    );
    assert_eq!(
        members(&pool, derived_id).await,
        members(&pool, published_bundle_version).await
    );
    let (created_by, via_agent): (Option<Uuid>, bool) = sqlx::query_as(
        "SELECT created_by, created_via_agent FROM compliance_bundle_versions WHERE id = $1",
    )
    .bind(derived_id)
    .fetch_one(&pool)
    .await
    .expect("derived row");
    assert_eq!(created_by, Some(token.owner));
    assert!(via_agent);
    assert_eq!(
        audit_rows(&pool, derived_id).await[0].2["operation"],
        "derive"
    );

    // A second derivation names only the open draft.
    let message = invalid_message(
        call_err(
            &author,
            &pool,
            "create_bundle_draft",
            json!({"mode": "derive", "bundle_id": published_bundle_id}),
        )
        .await,
    );
    assert!(message.starts_with("draft_exists:"), "{message}");
    assert!(message.contains(&derived_id.to_string()), "{message}");

    // A used name is a conflict.
    let name: String = sqlx::query_scalar("SELECT name FROM compliance_bundles WHERE id = $1")
        .bind(bundle_id)
        .fetch_one(&pool)
        .await
        .expect("bundle name");
    let before = counts(&pool).await;
    let message = invalid_message(
        call_err(
            &author,
            &pool,
            "create_bundle_draft",
            new_bundle(&name, &[]),
        )
        .await,
    );
    assert!(message.starts_with("name_conflict:"), "{message}");
    assert_eq!(counts(&pool).await, before);
}

#[sqlx::test]
async fn membership_updates_are_exact_validated_and_revisioned(pool: PgPool) {
    let (token, author) = admin_author(&pool, BUNDLES).await;
    let (first_lineage, first) = published_policy(&pool, &unique("first")).await;
    let first_draft = newer_policy_draft(&pool, first_lineage).await;
    let (second_lineage, second) = draft_policy(&pool, &unique("second")).await;
    let (_, deprecated) = published_policy(&pool, &unique("deprecated")).await;
    sqlx::query(
        "UPDATE deployment_policy_versions SET publication_state = 'deprecated' WHERE id = $1",
    )
    .bind(deprecated)
    .execute(&pool)
    .await
    .expect("deprecate version");

    let created = call(
        &author,
        &pool,
        "create_bundle_draft",
        new_bundle(&unique("update"), &[first]),
    )
    .await;
    let bundle_id = uuid(&created["bundle_id"]);
    let version_id = uuid(&created["version_id"]);
    let initial = created["draft_revision"].as_i64().expect("revision");

    // Invalid memberships fail without a write.
    let before = counts(&pool).await;
    for (list, code) in [
        (vec![first, first], "duplicate_policy_version"),
        (vec![first, Uuid::new_v4()], "unknown_policy_version"),
        (vec![first, first_draft], "duplicate_policy_lineage"),
        (vec![second, deprecated], "member_never_publishable"),
        (vec![first_lineage], "unknown_policy_version"),
    ] {
        let message = invalid_message(
            call_err(
                &author,
                &pool,
                "update_bundle_draft",
                json!({"version_id": version_id, "draft_revision": initial,
                       "policy_version_ids": list}),
            )
            .await,
        );
        assert!(message.starts_with(code), "{code}: {message}");
    }
    assert_eq!(counts(&pool).await, before);
    assert_eq!(bundle_revision(&pool, version_id).await, initial);

    // A valid update writes the exact ordered versions and keeps the lineage
    // rows consistent.
    let updated = call(
        &author,
        &pool,
        "update_bundle_draft",
        json!({"version_id": version_id, "draft_revision": initial,
               "description": "Refined by the agent",
               "policy_version_ids": [second, first]}),
    )
    .await;
    assert_eq!(updated["changed"], true);
    assert_eq!(
        updated["changed_fields"],
        json!(["description", "policy_version_ids"])
    );
    let revised = updated["draft_revision"].as_i64().expect("revision");
    assert!(revised > initial, "{initial} -> {revised}");
    assert_eq!(revised, bundle_revision(&pool, version_id).await);
    assert_eq!(members(&pool, version_id).await, vec![second, first]);
    assert_eq!(
        lineages(&pool, bundle_id).await,
        sorted(vec![first_lineage, second_lineage])
    );
    let (description, digest): (Option<String>, String) = sqlx::query_as(
        "SELECT description, semantic_digest FROM compliance_bundle_versions WHERE id = $1",
    )
    .bind(version_id)
    .fetch_one(&pool)
    .await
    .expect("updated row");
    assert_eq!(description.as_deref(), Some("Refined by the agent"));
    assert_ne!(digest, "pending");
    let audits = audit_rows(&pool, version_id).await;
    let update_audit = audits.last().expect("update audit");
    assert_eq!(update_audit.0, "bundle_draft_updated");
    assert_eq!(update_audit.1, Some(token.id));
    assert_eq!(update_audit.2["previous_draft_revision"], initial);
    assert_eq!(update_audit.2["draft_revision"], revised);
    assert!(!update_audit.2.to_string().contains("Refined by the agent"));

    // A stale revision fails and writes nothing.
    let before = counts(&pool).await;
    let message = invalid_message(
        call_err(
            &author,
            &pool,
            "update_bundle_draft",
            json!({"version_id": version_id, "draft_revision": initial,
                   "policy_version_ids": [first]}),
        )
        .await,
    );
    assert!(message.starts_with("stale_revision:"), "{message}");
    assert_eq!(counts(&pool).await, before);
    assert_eq!(members(&pool, version_id).await, vec![second, first]);

    // A no-op keeps the revision and writes no audit event.
    let unchanged = call(
        &author,
        &pool,
        "update_bundle_draft",
        json!({"version_id": version_id, "draft_revision": revised,
               "description": "Refined by the agent",
               "policy_version_ids": [second, first]}),
    )
    .await;
    assert_eq!(unchanged["changed"], false);
    assert_eq!(unchanged["draft_revision"], revised);
    assert_eq!(counts(&pool).await, before);

    // A browser edit of the same draft increments the revision and is
    // audited, so the agent's next update with the old revision is stale.
    let name: String = sqlx::query_scalar("SELECT name FROM compliance_bundles WHERE id = $1")
        .bind(bundle_id)
        .fetch_one(&pool)
        .await
        .expect("bundle name");
    update_bundle(
        &pool,
        bundle_id,
        UpdateComplianceBundleRequest {
            name,
            framework: "NIST 800-53".into(),
            version: Some("rev5".into()),
            description: Some("Edited in the browser".into()),
            required_envs: Vec::new(),
            policy_ids: vec![second_lineage, first_lineage],
            requirement_version_ids: Vec::new(),
        },
        Some(token.owner),
    )
    .await
    .expect("browser edit")
    .expect("bundle exists");
    assert!(bundle_revision(&pool, version_id).await > revised);
    let browser_audit = audit_rows(&pool, version_id)
        .await
        .pop()
        .expect("browser audit");
    assert_eq!(browser_audit.0, "bundle_draft_updated");
    assert_eq!(browser_audit.1, None);
    assert_eq!(browser_audit.2["operation"], "browser_update");
    let message = invalid_message(
        call_err(
            &author,
            &pool,
            "update_bundle_draft",
            json!({"version_id": version_id, "draft_revision": revised, "description": "x"}),
        )
        .await,
    );
    assert!(message.starts_with("stale_revision:"), "{message}");
}

#[sqlx::test]
async fn published_bundles_and_other_users_drafts_cannot_be_updated(pool: PgPool) {
    let (_, author) = admin_author(&pool, BUNDLES).await;
    let (_, other_author) = admin_author(&pool, BUNDLES).await;
    let (lineage, version) = published_policy(&pool, &unique("member")).await;
    let (_, published_version) = published_bundle(&pool, &[lineage]).await;
    let accepted_revision = bundle_revision(&pool, published_version).await;
    let before = counts(&pool).await;
    let message = invalid_message(
        call_err(
            &author,
            &pool,
            "update_bundle_draft",
            json!({"version_id": published_version, "draft_revision": accepted_revision,
                   "policy_version_ids": []}),
        )
        .await,
    );
    assert!(message.starts_with("immutable_version:"), "{message}");
    assert_eq!(counts(&pool).await, before);
    assert_eq!(
        bundle_revision(&pool, published_version).await,
        accepted_revision
    );
    assert_eq!(members(&pool, published_version).await, vec![version]);

    let theirs = call(
        &other_author,
        &pool,
        "create_bundle_draft",
        new_bundle(&unique("theirs"), &[version]),
    )
    .await;
    let before = counts(&pool).await;
    let error = call_err(
        &author,
        &pool,
        "update_bundle_draft",
        json!({"version_id": theirs["version_id"], "draft_revision": theirs["draft_revision"],
               "description": "Hijacked"}),
    )
    .await;
    assert!(
        matches!(&error, ToolError::Forbidden(message)
            if message == "bundle draft is owned by another user"),
        "{error:?}"
    );
    assert_eq!(counts(&pool).await, before);

    let unknown = call_err(
        &author,
        &pool,
        "update_bundle_draft",
        json!({"version_id": Uuid::new_v4(), "draft_revision": 1, "description": "x"}),
    )
    .await;
    assert!(matches!(unknown, ToolError::NotFound), "{unknown:?}");
}

#[sqlx::test]
async fn concurrent_updates_of_one_draft_have_exactly_one_winner(pool: PgPool) {
    let (_, author) = admin_author(&pool, BUNDLES).await;
    let (_, first) = published_policy(&pool, &unique("first")).await;
    let (_, second) = published_policy(&pool, &unique("second")).await;
    let created = call(
        &author,
        &pool,
        "create_bundle_draft",
        new_bundle(&unique("race"), &[first]),
    )
    .await;
    let version_id = uuid(&created["version_id"]);
    let revision = created["draft_revision"].clone();
    let (left, right) = tokio::time::timeout(StdDuration::from_secs(60), async {
        tokio::join!(
            call_tool(
                &pool,
                &author,
                "update_bundle_draft",
                json!({"version_id": version_id, "draft_revision": revision,
                       "description": "left", "policy_version_ids": [first, second]}),
            ),
            call_tool(
                &pool,
                &author,
                "update_bundle_draft",
                json!({"version_id": version_id, "draft_revision": revision,
                       "description": "right", "policy_version_ids": [second, first]}),
            ),
        )
    })
    .await
    .expect("concurrent updates finish without a deadlock");
    let outcomes = [left, right];
    let winners = outcomes.iter().filter(|outcome| outcome.is_ok()).count();
    assert_eq!(winners, 1, "{outcomes:?}");
    for outcome in &outcomes {
        if let Err(error) = outcome {
            assert!(
                matches!(error, ToolError::InvalidArguments(message)
                    if message.starts_with("stale_revision:")),
                "{error:?}"
            );
        }
    }
    let (description, stored) = (
        sqlx::query_scalar::<_, Option<String>>(
            "SELECT description FROM compliance_bundle_versions WHERE id = $1",
        )
        .bind(version_id)
        .fetch_one(&pool)
        .await
        .expect("description"),
        members(&pool, version_id).await,
    );
    match description.as_deref() {
        Some("left") => assert_eq!(stored, vec![first, second]),
        Some("right") => assert_eq!(stored, vec![second, first]),
        other => panic!("unexpected description {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Authorization
// ---------------------------------------------------------------------------

#[sqlx::test]
async fn demoted_owner_is_rejected_inside_the_transaction(pool: PgPool) {
    let (token, author) = admin_author(&pool, BUNDLES).await;
    let (_, member) = published_policy(&pool, &unique("member")).await;
    let created = call(
        &author,
        &pool,
        "create_bundle_draft",
        new_bundle(&unique("demotion"), &[member]),
    )
    .await;
    // The caller was admitted while the owner was Admin; the owner loses
    // Admin before the write transaction starts.
    sync_user_role(&pool, token.owner, AuthRole::Operator)
        .await
        .expect("demote owner");
    let before = counts(&pool).await;
    for (tool, arguments) in [
        (
            "create_bundle_draft",
            new_bundle(&unique("after-demotion"), &[]),
        ),
        (
            "update_bundle_draft",
            json!({"version_id": created["version_id"],
                   "draft_revision": created["draft_revision"], "description": "x"}),
        ),
    ] {
        let error = call_err(&author, &pool, tool, arguments).await;
        assert!(
            matches!(error, ToolError::Forbidden(_)),
            "{tool}: {error:?}"
        );
    }
    assert_eq!(counts(&pool).await, before);

    // A later admission sees the demotion before any write.
    let demoted = caller(&token, BUNDLES, false);
    let error = call_err(
        &demoted,
        &pool,
        "create_bundle_draft",
        new_bundle(&unique("later"), &[]),
    )
    .await;
    assert!(matches!(error, ToolError::Forbidden(_)), "{error:?}");
    assert_eq!(counts(&pool).await, before);
}

#[sqlx::test]
async fn bundle_tools_require_their_scope_even_without_tools_list(pool: PgPool) {
    let owner = user(&pool, AuthRole::Admin).await;
    let reader = issue(&pool, owner, &["bundles:read"]).await;
    let before = counts(&pool).await;
    let read_only = caller(&reader, &["bundles:read"], true);
    for (tool, arguments) in [
        ("create_bundle_draft", new_bundle(&unique("no-scope"), &[])),
        (
            "update_bundle_draft",
            json!({"version_id": Uuid::new_v4(), "draft_revision": 1}),
        ),
        (
            "preview_bundle_impact",
            json!({"bundle_version_id": Uuid::new_v4(), "system_id": Uuid::new_v4(),
                   "enforcement_mode": "enforce"}),
        ),
    ] {
        let error = call_err(&read_only, &pool, tool, arguments).await;
        assert!(
            matches!(error, ToolError::Forbidden(_)),
            "{tool}: {error:?}"
        );
    }

    // A grant that claims the scope does not help when the token row lacks
    // it: the write transaction checks the row.
    let forged = caller(&reader, BUNDLES, true);
    let error = call_err(
        &forged,
        &pool,
        "create_bundle_draft",
        new_bundle(&unique("forged"), &[]),
    )
    .await;
    assert!(matches!(error, ToolError::Forbidden(_)), "{error:?}");
    assert_eq!(counts(&pool).await, before);

    // Non-Admin owners cannot hold the scope at all.
    for role in [AuthRole::Operator, AuthRole::Viewer] {
        let owner = user(&pool, role).await;
        let environment = environment(&pool).await;
        insert_user_environment_membership(&pool, owner, environment)
            .await
            .expect("assign membership");
        let refused = token_service::create(
            &pool,
            owner,
            &token_request(&unique("refused"), environment, BUNDLES),
            &settings(),
        )
        .await;
        assert!(refused.is_err(), "{role:?} received bundles:draft");
    }
}

// ---------------------------------------------------------------------------
// Impact preview
// ---------------------------------------------------------------------------

#[sqlx::test]
async fn preview_reports_differences_and_review_state_without_writing(pool: PgPool) {
    // A Viewer may hold compliance:preview.
    let viewer = user(&pool, AuthRole::Viewer).await;
    let token = issue(&pool, viewer, PREVIEW).await;
    let previewer = caller(&token, PREVIEW, false);
    let admin = AuthoringActor::browser(user(&pool, AuthRole::Admin).await);
    let first_system = system(&pool, &unique("alpha"), token.environment).await;
    let second_system = system(&pool, &unique("beta"), token.environment).await;

    // The environment enforces a published bundle with one policy.
    let (assigned_lineage, assigned_version) = published_policy(&pool, &unique("assigned")).await;
    let (assigned_bundle, assigned_bundle_version) =
        published_bundle(&pool, &[assigned_lineage]).await;
    assign_environment(
        &pool,
        assigned_bundle,
        assigned_bundle_version,
        token.environment,
    )
    .await;

    // Candidate A: a new bundle with an untrusted draft member and a
    // published member.
    let (_, untrusted_member) = draft_policy(&pool, &unique("untrusted")).await;
    let (_, published_member) = published_policy(&pool, &unique("published")).await;
    let candidate = create_bundle_lineage_draft(
        &pool,
        &admin,
        &NewBundleLineage {
            name: unique("candidate"),
            description: None,
            framework: "CF-TEST".into(),
            framework_version: None,
            policy_version_ids: vec![untrusted_member, published_member],
        },
    )
    .await
    .expect("candidate bundle");

    let before = preview_side_effects(&pool).await;
    let preview = call(
        &previewer,
        &pool,
        "preview_bundle_impact",
        json!({"bundle_version_id": candidate.version_id, "system_id": first_system,
               "enforcement_mode": "report_only"}),
    )
    .await;
    assert_eq!(preview["scope_type"], "system");
    assert_eq!(preview["enforcement_mode"], "report_only");
    assert_eq!(preview["assignable_now"], false);
    assert_eq!(preview["truncated"], false);
    let systems = preview["systems"].as_array().expect("systems");
    assert_eq!(systems.len(), 1);
    let impact = &systems[0];
    assert_eq!(uuid(&impact["system_id"]), first_system);
    assert_eq!(impact["current"]["status"], "resolved");
    assert_eq!(
        pairs(&impact["current"]["policies"]),
        vec![(assigned_lineage, assigned_version)]
    );
    assert_eq!(impact["proposed"]["status"], "resolved");
    let added = impact["added"].as_array().expect("added");
    assert_eq!(
        added
            .iter()
            .map(|policy| uuid(&policy["policy_version_id"]))
            .collect::<Vec<_>>(),
        vec![untrusted_member, published_member]
    );
    assert!(
        added
            .iter()
            .all(|policy| policy["enforcement_mode"] == "report_only")
    );
    assert_eq!(impact["removed"], json!([]));
    assert_eq!(impact["changed"], json!([]));
    // The candidate's review state is reported, never hidden.
    let member = preview["members"]
        .as_array()
        .expect("members")
        .iter()
        .find(|member| uuid(&member["policy_version_id"]) == untrusted_member)
        .expect("untrusted member listed");
    assert_eq!(member["trust_state"], "untrusted");
    assert_eq!(member["publication_state"], "draft");
    assert_eq!(preview["facts"]["untrusted_members"], 1);
    assert_eq!(preview["facts"]["draft_members"], 1);
    assert_eq!(preview["facts"]["added_policies"], 2);
    assert!(
        preview["warnings"]
            .as_array()
            .expect("warnings")
            .iter()
            .any(|warning| warning
                .as_str()
                .is_some_and(|text| text.contains("not trusted")))
    );
    assert!(
        !preview["requires_evaluation"]
            .as_array()
            .expect("requires_evaluation")
            .is_empty()
    );
    assert!(preview["coverage"]["total_requirements"].is_i64());
    assert!(preview.get("passed").is_none() && preview.get("compliant").is_none());
    assert_eq!(
        preview_side_effects(&pool).await,
        before,
        "a preview writes, assigns and schedules nothing"
    );

    // Candidate B: a derived draft of the assigned bundle that moves its
    // policy to a newer draft version. Assigning it to the environment would
    // replace the existing assignment, so the policy changes version.
    let derived = derive_bundle_draft(&pool, &admin, assigned_bundle)
        .await
        .expect("derive assigned bundle");
    let newer = newer_policy_draft(&pool, assigned_lineage).await;
    update_bundle_draft(
        &pool,
        &admin,
        derived.version_id,
        derived.draft_revision,
        &BundleDraftChanges {
            policy_version_ids: Some(vec![newer]),
            ..BundleDraftChanges::default()
        },
    )
    .await
    .expect("update derived draft");
    let before = preview_side_effects(&pool).await;
    let preview = call(
        &previewer,
        &pool,
        "preview_bundle_impact",
        json!({"bundle_version_id": derived.version_id, "environment_id": token.environment,
               "enforcement_mode": "enforce"}),
    )
    .await;
    assert_eq!(preview["scope_type"], "environment");
    let systems = preview["systems"].as_array().expect("systems");
    let mut listed = systems
        .iter()
        .map(|system| uuid(&system["system_id"]))
        .collect::<Vec<_>>();
    listed.sort();
    assert_eq!(listed, sorted(vec![first_system, second_system]));
    for impact in systems {
        assert_eq!(impact["added"], json!([]), "{impact}");
        assert_eq!(impact["removed"], json!([]), "{impact}");
        let changed = impact["changed"].as_array().expect("changed");
        assert_eq!(changed.len(), 1, "{impact}");
        assert_eq!(uuid(&changed[0]["policy_lineage_id"]), assigned_lineage);
        assert_eq!(uuid(&changed[0]["current_version_id"]), assigned_version);
        assert_eq!(uuid(&changed[0]["proposed_version_id"]), newer);
    }
    assert_eq!(preview["facts"]["systems_with_changes"], 2);
    assert_eq!(
        preview_side_effects(&pool).await,
        before,
        "a preview writes, assigns and schedules nothing"
    );
}

#[sqlx::test]
async fn preview_hides_targets_outside_the_token_environments(pool: PgPool) {
    let viewer = user(&pool, AuthRole::Viewer).await;
    let token = issue(&pool, viewer, PREVIEW).await;
    let previewer = caller(&token, PREVIEW, false);
    let admin = AuthoringActor::browser(user(&pool, AuthRole::Admin).await);
    let (_, member) = published_policy(&pool, &unique("member")).await;
    let candidate = create_bundle_lineage_draft(
        &pool,
        &admin,
        &NewBundleLineage {
            name: unique("hidden"),
            description: None,
            framework: "CF-TEST".into(),
            framework_version: None,
            policy_version_ids: vec![member],
        },
    )
    .await
    .expect("candidate bundle");
    let inside = system(&pool, &unique("inside"), token.environment).await;
    let other_environment = environment(&pool).await;
    let outside = system(&pool, &unique("outside"), other_environment).await;

    let before = preview_side_effects(&pool).await;
    for arguments in [
        json!({"bundle_version_id": candidate.version_id, "system_id": outside,
               "enforcement_mode": "enforce"}),
        json!({"bundle_version_id": candidate.version_id, "system_id": Uuid::new_v4(),
               "enforcement_mode": "enforce"}),
        json!({"bundle_version_id": candidate.version_id, "environment_id": other_environment,
               "enforcement_mode": "enforce"}),
        json!({"bundle_version_id": Uuid::new_v4(), "system_id": inside,
               "enforcement_mode": "enforce"}),
    ] {
        let error = call_err(
            &previewer,
            &pool,
            "preview_bundle_impact",
            arguments.clone(),
        )
        .await;
        assert!(
            matches!(error, ToolError::NotFound),
            "{arguments}: {error:?}"
        );
    }
    let error = call_err(
        &previewer,
        &pool,
        "preview_bundle_impact",
        json!({"bundle_version_id": candidate.version_id, "system_id": inside,
               "environment_id": token.environment, "enforcement_mode": "enforce"}),
    )
    .await;
    assert!(matches!(error, ToolError::InvalidArguments(_)), "{error:?}");

    // The environment preview lists only the token environment's systems.
    let preview = call(
        &previewer,
        &pool,
        "preview_bundle_impact",
        json!({"bundle_version_id": candidate.version_id, "environment_id": token.environment,
               "enforcement_mode": "enforce"}),
    )
    .await;
    let listed = preview["systems"]
        .as_array()
        .expect("systems")
        .iter()
        .map(|system| uuid(&system["system_id"]))
        .collect::<Vec<_>>();
    assert_eq!(listed, vec![inside]);
    assert!(!preview.to_string().contains(&outside.to_string()));
    assert_eq!(preview_side_effects(&pool).await, before);
}

// ---------------------------------------------------------------------------
// HTTP through /mcp
// ---------------------------------------------------------------------------

struct Reply {
    status: u16,
    text: String,
    body: Value,
}

struct Server {
    base: String,
    client: Client,
}

async fn start(pool: &PgPool) -> Server {
    let state = CFState::new(
        pool.clone(),
        ServerConfig {
            mcp_enable: true,
            mcp_public_url: Some("https://forge.example.com/mcp".to_owned()),
            ..ServerConfig::default()
        },
        Arc::new(QueueNotifier::new()),
        BackgroundJobRegistry::new(),
    );
    let app = Router::new()
        .route(
            "/mcp",
            post(mcp::handle).layer(DefaultBodyLimit::max(mcp::MAX_BODY_BYTES)),
        )
        .with_state(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind MCP test server");
    let address = listener.local_addr().expect("read test server address");
    tokio::spawn(async move {
        axum::serve(listener, app)
            .await
            .expect("serve MCP test API")
    });
    Server {
        base: format!("http://{address}"),
        client: Client::new(),
    }
}

impl Server {
    async fn call(&self, token: &Token, tool: &str, arguments: Value) -> Reply {
        let body = json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call",
                          "params": {"name": tool, "arguments": arguments}});
        let response = self
            .client
            .post(format!("{}/mcp", self.base))
            .header("content-type", "application/json")
            .header("authorization", format!("Bearer {}", token.secret))
            .body(body.to_string())
            .send()
            .await
            .expect("send request");
        let status = response.status().as_u16();
        let text = response.text().await.expect("read response body");
        let body = serde_json::from_str(&text).unwrap_or(Value::Null);
        Reply { status, text, body }
    }
}

fn structured(reply: &Reply) -> Value {
    assert_eq!(reply.status, 200, "{}", reply.text);
    assert_eq!(reply.body["result"]["isError"], false, "{}", reply.text);
    reply.body["result"]["structuredContent"].clone()
}

/// One-policy interchange document with explicit portable identities.
fn interchange(lineage_id: Uuid, version_id: Uuid, name: &str) -> String {
    json!({
        "schema": "urn:crystal-forge:policy-set:1",
        "version": "1",
        "policies": [{
            "lineage_id": lineage_id,
            "version_id": version_id,
            "version": "1.0.0",
            "name": name,
            "policy_type": "custom_check",
            "config": {
                "mode": "all",
                "rules": [{"field_name": "ssh", "expression": "config.services.openssh.enable",
                           "strict": true}]
            },
            "enabled_by_default": true
        }]
    })
    .to_string()
}

/// Acceptance criterion #5: list and inspect policies, create and update a
/// policy draft, preview and import, create a bundle draft with exact
/// versions, preview it on an authorized system, confirm that nothing was
/// assigned or scheduled, revoke the token and confirm that mutations stop.
#[sqlx::test]
async fn full_agent_workflow_over_http(pool: PgPool) {
    let owner = user(&pool, AuthRole::Admin).await;
    let token = issue(
        &pool,
        owner,
        &[
            "policies:read",
            "policies:draft",
            "policies:import",
            "bundles:read",
            "bundles:draft",
            "compliance:preview",
        ],
    )
    .await;
    let server = start(&pool).await;
    let host = system(&pool, &unique("workflow"), token.environment).await;
    let published_name = unique("baseline");
    let (published_lineage, published_version) = published_policy(&pool, &published_name).await;
    let (assigned_bundle, assigned_bundle_version) =
        published_bundle(&pool, &[published_lineage]).await;
    assign_environment(
        &pool,
        assigned_bundle,
        assigned_bundle_version,
        token.environment,
    )
    .await;

    // 1. List and inspect policies.
    let listed = structured(
        &server
            .call(
                &token,
                "list_compliance_policies",
                json!({"query": published_name}),
            )
            .await,
    );
    assert_eq!(
        listed["items"][0]["current_published_version"]["version_id"],
        published_version.to_string()
    );
    let inspected = structured(
        &server
            .call(
                &token,
                "get_compliance_policy",
                json!({"version_id": published_version}),
            )
            .await,
    );
    assert_eq!(inspected["version"]["publication_state"], "accepted");

    // 2. Create and update a policy draft.
    let created = structured(
        &server
            .call(
                &token,
                "create_policy_draft",
                json!({"mode": "new_lineage", "name": unique("agent-policy"),
                       "policy_type": "custom_check",
                       "config": {"mode": "all", "rules": []}}),
            )
            .await,
    );
    let drafted = uuid(&created["version_id"]);
    let updated = structured(
        &server
            .call(
                &token,
                "update_policy_draft",
                json!({"version_id": drafted, "draft_revision": created["draft_revision"],
                       "description": "Agent check"}),
            )
            .await,
    );
    assert_eq!(updated["changed"], true);

    // 3. Preview and import a policy document.
    let imported_version = Uuid::new_v4();
    let document = interchange(Uuid::new_v4(), imported_version, &unique("imported"));
    let preview = structured(
        &server
            .call(
                &token,
                "preview_policy_import",
                json!({"document": document, "format": "json"}),
            )
            .await,
    );
    assert_eq!(preview["source_sha256"], source_sha256(document.as_bytes()));
    let imported = structured(
        &server
            .call(
                &token,
                "import_policy_draft",
                json!({"document": document, "format": "json",
                       "expected_source_sha256": preview["source_sha256"]}),
            )
            .await,
    );
    assert_eq!(
        imported["policies"][0]["version_id"],
        imported_version.to_string()
    );

    // 4. Create a bundle draft with exact versions.
    let exact = vec![drafted, imported_version, published_version];
    let bundle = structured(
        &server
            .call(
                &token,
                "create_bundle_draft",
                new_bundle(&unique("workflow-bundle"), &exact),
            )
            .await,
    );
    let bundle_version = uuid(&bundle["version_id"]);
    let read = structured(
        &server
            .call(
                &token,
                "get_compliance_bundle",
                json!({"version_id": bundle_version}),
            )
            .await,
    );
    let read_members = read["version"]["policies"]
        .as_array()
        .expect("bundle policies")
        .iter()
        .map(|policy| uuid(&policy["policy_version_id"]))
        .collect::<Vec<_>>();
    assert_eq!(read_members, exact);

    // 5. Preview the bundle on the authorized system.
    let before = preview_side_effects(&pool).await;
    let impact = structured(
        &server
            .call(
                &token,
                "preview_bundle_impact",
                json!({"bundle_version_id": bundle_version, "system_id": host,
                       "enforcement_mode": "report_only"}),
            )
            .await,
    );
    let system_impact = &impact["systems"][0];
    assert_eq!(uuid(&system_impact["system_id"]), host);
    let added = system_impact["added"]
        .as_array()
        .expect("added")
        .iter()
        .map(|policy| uuid(&policy["policy_version_id"]))
        .collect::<Vec<_>>();
    assert_eq!(added, vec![drafted, imported_version]);
    // The published member is already enforced through the environment; the
    // more specific report-only candidate changes only its mode.
    let changed = system_impact["changed"].as_array().expect("changed");
    assert_eq!(changed.len(), 1, "{system_impact}");
    assert_eq!(uuid(&changed[0]["policy_lineage_id"]), published_lineage);
    assert_eq!(uuid(&changed[0]["proposed_version_id"]), published_version);
    assert_eq!(changed[0]["current_enforcement_mode"], "enforce");
    assert_eq!(changed[0]["proposed_enforcement_mode"], "report_only");
    assert_eq!(impact["facts"]["draft_members"], 2);
    assert!(impact["requires_evaluation"].is_array());

    // 6. Nothing was assigned or scheduled.
    assert_eq!(preview_side_effects(&pool).await, before);
    let candidate_assignments: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM compliance_bundle_assignment_versions WHERE bundle_version_id = $1",
    )
    .bind(bundle_version)
    .fetch_one(&pool)
    .await
    .expect("candidate assignments");
    assert_eq!(candidate_assignments, 0);

    // 7. Revocation stops every mutation.
    token_service::revoke(&pool, owner, token.id)
        .await
        .expect("revoke token");
    let before = counts(&pool).await;
    let revision = bundle_revision(&pool, bundle_version).await;
    for (tool, arguments) in [
        (
            "update_bundle_draft",
            json!({"version_id": bundle_version, "draft_revision": revision,
                   "description": "After revocation"}),
        ),
        ("create_bundle_draft", new_bundle(&unique("revoked"), &[])),
    ] {
        let reply = server.call(&token, tool, arguments).await;
        assert_eq!(reply.status, 401, "{tool}: {}", reply.text);
    }
    assert_eq!(counts(&pool).await, before);
    assert_eq!(bundle_revision(&pool, bundle_version).await, revision);
}
