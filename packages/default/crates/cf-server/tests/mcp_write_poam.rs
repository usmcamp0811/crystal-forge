//! POA&M write MCP tool regressions (TASK-534.5).
//!
//! Each SQLx test owns an isolated database with the full production
//! migrations. Tokens are issued through the access-token service and admitted
//! with `validate_presented_token`, as the MCP transport does, so the in-
//! transaction token-row check sees real rows. Tests call the tool functions
//! directly. The system-move race drives the real system edit handler over
//! HTTP with an Admin browser session.

use std::sync::Arc;
use std::time::Duration;

use axum::{Router, routing::patch};
use chrono::{NaiveDate, Utc};
use crystal_forge::api::models::CreateAccessTokenRequest;
use crystal_forge::auth::authorization::CredentialIdentity;
use crystal_forge::auth::session::{
    CSRF_COOKIE_NAME, CSRF_HEADER_NAME, SESSION_COOKIE_NAME, hash_token,
};
use crystal_forge::handlers::{agent_request::CFState, api::systems};
use crystal_forge::mcp::poam_write::{update_poam_progress, verify_poam};
use crystal_forge::mcp::tools::{ToolCaller, ToolError};
use crystal_forge::models::auth_identity::AuthRole;
use crystal_forge::models::poam::{
    AddCveFindingRequest, AddNoteRequest, CreateCvePoamRequest, CveObservationReference, PoamRisk,
    UpdateMilestoneRequest,
};
use crystal_forge::queries::auth_identity::{
    clear_user_environment_memberships, create_user_session, insert_user_environment_membership,
    sync_user_role,
};
use crystal_forge::queries::users::insert_user;
use crystal_forge::services::access_tokens::{
    self as token_service, AccessTokenSettings, validate_presented_token,
};
use crystal_forge::services::poam::{self as poam_service, PoamActor, SystemClock};
use crystal_forge::{
    config::ServerConfig, queue::QueueNotifier, server::jobs::BackgroundJobRegistry,
};
use reqwest::Client;
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

const WRITE: &[&str] = &["poams:read", "poams:write"];
const WRITE_VERIFY: &[&str] = &["poams:read", "poams:write", "poams:verify"];
const READ: &[&str] = &["poams:read"];

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

async fn environment(pool: &PgPool, label: &str) -> Uuid {
    let short = Uuid::new_v4().simple().to_string();
    sqlx::query_scalar("INSERT INTO environments(name) VALUES($1) RETURNING id")
        .bind(format!("{label}-{}", &short[..8]))
        .fetch_one(pool)
        .await
        .expect("insert environment")
}

async fn user(pool: &PgPool, role: AuthRole, environments: &[Uuid]) -> Uuid {
    let suffix = Uuid::new_v4().simple().to_string();
    let user = insert_user(
        pool,
        &format!("mcp-write-{suffix}@example.invalid"),
        Some("MCP Write Test"),
    )
    .await
    .expect("insert user");
    sync_user_role(pool, user.id, role)
        .await
        .expect("assign role");
    for environment in environments {
        insert_user_environment_membership(pool, user.id, *environment)
            .await
            .expect("assign membership");
    }
    user.id
}

/// One issued token: the request caller and the token name snapshot.
struct Token {
    caller: ToolCaller,
    name: String,
}

/// Issues a token through the service and admits it like the transport.
async fn issue(pool: &PgPool, owner: Uuid, scopes: &[&str], environments: &[Uuid]) -> Token {
    let name = format!("agent-{}", &Uuid::new_v4().simple().to_string()[..8]);
    let request: CreateAccessTokenRequest = serde_json::from_value(json!({
        "name": name,
        "audience": "mcp",
        "expires_in_days": 30,
        "environment_ids": environments.iter().map(Uuid::to_string).collect::<Vec<_>>(),
        "scopes": scopes,
    }))
    .expect("token request");
    let issued = token_service::create(pool, owner, &request, &settings())
        .await
        .expect("issue token");
    let validated = validate_presented_token(pool, issued.secret.expose(), Utc::now(), &settings())
        .await
        .expect("admit token");
    Token {
        caller: ToolCaller {
            token_id: issued.metadata.id,
            owner_user_id: validated.owner_user_id,
            owner_is_admin: validated.owner_is_admin,
            owner_environment_ids: validated.owner_environment_ids,
            grant: validated.grant,
        },
        name,
    }
}

/// Creates a managed system with a running report and one sealed scan that
/// finds `CVE-2099-0701` in `pkg` and `CVE-2099-0702` in `pkg2`.
async fn scanned_host(pool: &PgPool, environment_id: Uuid) -> Vec<CveObservationReference> {
    let suffix = Uuid::new_v4().simple().to_string();
    let hostname = format!("mcp-write-{suffix}");
    let flake: i32 =
        sqlx::query_scalar("INSERT INTO flakes(name,repo_url) VALUES($1,$2) RETURNING id")
            .bind(format!("mcp-write-flake-{suffix}"))
            .bind(format!("https://example.invalid/{suffix}.git"))
            .fetch_one(pool)
            .await
            .expect("insert flake");
    let commit: i32 = sqlx::query_scalar(
        "INSERT INTO commits(flake_id,git_commit_hash,commit_timestamp) VALUES($1,$2,now()) RETURNING id",
    )
    .bind(flake)
    .bind(format!("{suffix}{}", &suffix[..8]))
    .fetch_one(pool)
    .await
    .expect("insert commit");
    let system_id: Uuid = sqlx::query_scalar(
        "INSERT INTO systems(hostname,is_active,public_key,derivation,flake_id,system_configuration_name,environment_id) \
         VALUES($1,true,$2,'test-derivation',$3,$1,$4) RETURNING id",
    )
    .bind(&hostname)
    .bind(format!("mcp-write-key-{suffix}"))
    .bind(flake)
    .bind(environment_id)
    .fetch_one(pool)
    .await
    .expect("insert system");
    let store_path = format!("/nix/store/{suffix}-mcp-write-system");
    let derivation_id: i32 = sqlx::query_scalar(
        "INSERT INTO derivations(commit_id,derivation_type,derivation_name,derivation_path, \
         status_id,completed_at,store_path) VALUES($1,'nixos',$2,$3,10,now(),$4) RETURNING id",
    )
    .bind(commit)
    .bind(&hostname)
    .bind(format!("{store_path}.drv"))
    .bind(&store_path)
    .fetch_one(pool)
    .await
    .expect("insert system derivation");
    sqlx::query(
        "INSERT INTO system_states(hostname,change_reason,store_path,generation, \
         generation_matches_current_store_path) VALUES($1,'startup',$2,1,true)",
    )
    .bind(&hostname)
    .bind(&store_path)
    .execute(pool)
    .await
    .expect("insert running report");
    let scan_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO cve_scans(id,derivation_id,scanner_name,status) VALUES($1,$2,'mcp-test','in_progress')",
    )
    .bind(scan_id)
    .bind(derivation_id)
    .execute(pool)
    .await
    .expect("insert scan");
    let mut references = Vec::new();
    for (cve_id, package) in [("CVE-2099-0701", "pkg"), ("CVE-2099-0702", "pkg2")] {
        sqlx::query(
            "INSERT INTO cves(id,cvss_v3_score,description) VALUES($1,5.0,'Write tool fixture') \
             ON CONFLICT(id) DO NOTHING",
        )
        .bind(cve_id)
        .execute(pool)
        .await
        .expect("insert CVE");
        let path = format!("/nix/store/{}-{package}.drv", Uuid::new_v4().simple());
        sqlx::query(
            "INSERT INTO cve_scan_vulnerability_observations( \
             scan_id,canonical_cve_id,canonical_package_name,observed_package_name, \
             observed_package_version,observed_derivation_path,is_whitelisted,detection_method) \
             VALUES($1,$2,$3,$3,'1.0',$4,false,'mcp-test')",
        )
        .bind(scan_id)
        .bind(cve_id)
        .bind(package)
        .bind(&path)
        .execute(pool)
        .await
        .expect("insert observation");
        references.push(CveObservationReference {
            system_id,
            scan_id,
            occurrence_derivation_path: path,
            canonical_cve_id: cve_id.into(),
            canonical_package_name: package.into(),
        });
    }
    sqlx::query(
        "UPDATE cve_scans SET status='completed',completed_at=now(),evidence_schema_version=1 WHERE id=$1",
    )
    .bind(scan_id)
    .execute(pool)
    .await
    .expect("seal scan");
    references
}

fn browser_admin(user_id: Uuid) -> PoamActor {
    PoamActor {
        credential: CredentialIdentity::BrowserSession,
        user_id,
        identifier: format!("mcp-write-admin-{user_id}@example.invalid"),
        is_admin: true,
        can_mutate: true,
        environment_ids: Vec::new(),
        request_origin: None,
    }
}

async fn create_poam(
    pool: &PgPool,
    actor: &PoamActor,
    observation: &CveObservationReference,
) -> (Uuid, i64) {
    let detail = poam_service::create_cve(
        pool,
        actor,
        CreateCvePoamRequest {
            observation: observation.clone(),
            title: "Upgrade vulnerable package".into(),
            plan: "Update the nixpkgs input".into(),
            owner: "MCP owner".into(),
            assignee: None,
            target_date: None,
            risk: PoamRisk::High,
            default_milestones: false,
            assignment_version_ids: Vec::new(),
        },
        &SystemClock,
    )
    .await
    .expect("create CVE POA&M");
    (detail.poam.id, detail.poam.revision)
}

/// Two environments, each with one scanned host, and a browser Admin.
struct Fleet {
    env_a: Uuid,
    env_b: Uuid,
    observations_a: Vec<CveObservationReference>,
    observations_b: Vec<CveObservationReference>,
    creator: PoamActor,
}

async fn fleet(pool: &PgPool) -> Fleet {
    let env_a = environment(pool, "mcp-write-a").await;
    let env_b = environment(pool, "mcp-write-b").await;
    let observations_a = scanned_host(pool, env_a).await;
    let observations_b = scanned_host(pool, env_b).await;
    let creator = browser_admin(user(pool, AuthRole::Admin, &[]).await);
    Fleet {
        env_a,
        env_b,
        observations_a,
        observations_b,
        creator,
    }
}

async fn progress(
    pool: &PgPool,
    token: &Token,
    poam_id: Uuid,
    revision: i64,
    action: Value,
) -> Result<Value, ToolError> {
    update_poam_progress(
        pool,
        &token.caller,
        json!({"poam_id": poam_id, "revision": revision, "action": action}),
    )
    .await
}

fn revision_of(result: &Value) -> i64 {
    result["revision"].as_i64().expect("revision")
}

async fn current_revision(pool: &PgPool, poam_id: Uuid) -> i64 {
    sqlx::query_scalar("SELECT revision FROM poams WHERE id=$1")
        .bind(poam_id)
        .fetch_one(pool)
        .await
        .expect("POA&M revision")
}

fn note() -> Value {
    json!({"type": "add_note", "text": "Bumped nixpkgs"})
}

// ---------------------------------------------------------------------------
// update_poam_progress
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "./migrations")]
async fn each_progress_action_records_token_attribution(pool: PgPool) {
    let fleet = fleet(&pool).await;
    let owner = user(&pool, AuthRole::Operator, &[fleet.env_a]).await;
    let token = issue(&pool, owner, WRITE, &[fleet.env_a]).await;
    let (poam_id, revision) = create_poam(&pool, &fleet.creator, &fleet.observations_a[0]).await;

    let noted = progress(&pool, &token, poam_id, revision, note())
        .await
        .expect("add note");
    assert_eq!(noted["poam_id"], json!(poam_id));
    assert_eq!(revision_of(&noted), revision + 1);
    assert_eq!(noted["status"], "open");
    assert_eq!(noted["changed"]["text"], "Bumped nixpkgs");

    let started = progress(
        &pool,
        &token,
        poam_id,
        revision_of(&noted),
        json!({"type": "set_status", "status": "in_progress", "note": "Working"}),
    )
    .await
    .expect("set status");
    assert_eq!(started["status"], "in_progress");
    assert_eq!(started["changed"]["from"], "open");

    let added = progress(
        &pool,
        &token,
        poam_id,
        revision_of(&started),
        json!({"type": "add_milestone", "title": "Deploy fix", "target_date": "2099-01-31"}),
    )
    .await
    .expect("add milestone");
    let milestone_id = added["changed"]["milestone_id"].clone();
    assert!(milestone_id.is_string());

    let updated = progress(
        &pool,
        &token,
        poam_id,
        revision_of(&added),
        json!({"type": "update_milestone", "milestone_id": milestone_id, "completed": true}),
    )
    .await
    .expect("update milestone");
    assert!(!updated["changed"]["new"]["completed_at"].is_null());

    let removed = progress(
        &pool,
        &token,
        poam_id,
        revision_of(&updated),
        json!({"type": "remove_milestone", "milestone_id": milestone_id}),
    )
    .await
    .expect("remove milestone");
    assert_eq!(revision_of(&removed), revision + 5);

    // Every write records the token on activity and audit rows, keeps the
    // owner as the actor, and snapshots the token name in audit metadata.
    let token_id = token.caller.token_id;
    let activity: Vec<(String, Option<Uuid>, Option<Uuid>)> = sqlx::query_as(
        "SELECT kind,actor_user_id,access_token_id FROM poam_activity \
         WHERE poam_id=$1 AND kind<>'created' ORDER BY created_at,id",
    )
    .bind(poam_id)
    .fetch_all(&pool)
    .await
    .expect("activity rows");
    assert_eq!(
        activity
            .iter()
            .map(|row| row.0.as_str())
            .collect::<Vec<_>>(),
        [
            "note",
            "status_changed",
            "milestone_added",
            "milestone_updated",
            "milestone_removed"
        ]
    );
    assert!(
        activity
            .iter()
            .all(|row| row.1 == Some(owner) && row.2 == Some(token_id))
    );
    let audits: Vec<(Option<Uuid>, Option<Uuid>, Value)> = sqlx::query_as(
        "SELECT actor_user_id,access_token_id,metadata FROM admin_audit_events \
         WHERE target=$1 AND action<>'poam_created'",
    )
    .bind(format!("poam:{poam_id}"))
    .fetch_all(&pool)
    .await
    .expect("audit rows");
    assert_eq!(audits.len(), 5);
    for (actor, access_token_id, metadata) in &audits {
        assert_eq!(*actor, Some(owner));
        assert_eq!(*access_token_id, Some(token_id));
        assert_eq!(metadata["via"], "mcp");
        assert_eq!(metadata["access_token_id"], json!(token_id));
        assert_eq!(metadata["access_token_name"], json!(token.name));
    }

    // The activity view names the token for the Web UI.
    let detail = poam_service::restricted_detail(
        &pool,
        &PoamActor {
            credential: CredentialIdentity::Restricted(token.caller.grant.clone()),
            user_id: owner,
            identifier: String::new(),
            is_admin: false,
            can_mutate: false,
            environment_ids: Vec::new(),
            request_origin: None,
        },
        poam_id,
        &SystemClock,
    )
    .await
    .expect("detail");
    let note_view = detail
        .activity
        .iter()
        .find(|item| item.kind == "note")
        .expect("note activity");
    assert_eq!(note_view.access_token_id, Some(token_id));
    assert_eq!(
        note_view.access_token_name.as_deref(),
        Some(token.name.as_str())
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn browser_writes_record_no_token(pool: PgPool) {
    let fleet = fleet(&pool).await;
    let (poam_id, revision) = create_poam(&pool, &fleet.creator, &fleet.observations_a[0]).await;
    poam_service::add_note(
        &pool,
        &fleet.creator,
        poam_id,
        AddNoteRequest {
            revision,
            text: "Browser note".into(),
        },
        &SystemClock,
    )
    .await
    .expect("browser note");
    let (activity_token, audit_token, metadata): (Option<Uuid>, Option<Uuid>, Value) =
        sqlx::query_as(
            "SELECT activity.access_token_id,audit.access_token_id,audit.metadata \
             FROM poam_activity activity \
             JOIN admin_audit_events audit ON audit.target='poam:'||activity.poam_id::text \
               AND audit.action='poam_note' \
             WHERE activity.poam_id=$1 AND activity.kind='note'",
        )
        .bind(poam_id)
        .fetch_one(&pool)
        .await
        .expect("browser note rows");
    assert_eq!(activity_token, None);
    assert_eq!(audit_token, None);
    assert!(metadata.get("via").is_none());
    assert!(metadata.get("access_token_name").is_none());
}

#[sqlx::test(migrations = "./migrations")]
async fn viewer_owner_and_missing_scope_are_forbidden(pool: PgPool) {
    let fleet = fleet(&pool).await;
    let (poam_id, revision) = create_poam(&pool, &fleet.creator, &fleet.observations_a[0]).await;

    let owner = user(&pool, AuthRole::Operator, &[fleet.env_a]).await;
    let read_only = issue(&pool, owner, READ, &[fleet.env_a]).await;
    assert!(matches!(
        progress(&pool, &read_only, poam_id, revision, note()).await,
        Err(ToolError::Forbidden(_))
    ));
    let write_only = issue(&pool, owner, WRITE, &[fleet.env_a]).await;
    assert!(matches!(
        verify_poam(
            &pool,
            &write_only.caller,
            json!({"poam_id": poam_id, "revision": revision})
        )
        .await,
        Err(ToolError::Forbidden(_))
    ));

    // The owner loses Operator after issuing; the next call is denied.
    sync_user_role(&pool, owner, AuthRole::Viewer)
        .await
        .expect("demote owner");
    assert!(matches!(
        progress(&pool, &write_only, poam_id, revision, note()).await,
        Err(ToolError::Forbidden(_))
    ));
    assert_eq!(current_revision(&pool, poam_id).await, revision);
}

#[sqlx::test(migrations = "./migrations")]
async fn revoked_token_is_forbidden_on_the_next_call(pool: PgPool) {
    let fleet = fleet(&pool).await;
    let owner = user(&pool, AuthRole::Operator, &[fleet.env_a]).await;
    let token = issue(&pool, owner, WRITE, &[fleet.env_a]).await;
    let (poam_id, revision) = create_poam(&pool, &fleet.creator, &fleet.observations_a[0]).await;
    let first = progress(&pool, &token, poam_id, revision, note())
        .await
        .expect("first write");

    // The admitted caller is reused: only the in-transaction token-row check
    // can see the revocation.
    token_service::revoke(&pool, owner, token.caller.token_id)
        .await
        .expect("revoke token");
    assert!(matches!(
        progress(&pool, &token, poam_id, revision_of(&first), note()).await,
        Err(ToolError::Forbidden(_))
    ));
    assert_eq!(current_revision(&pool, poam_id).await, revision_of(&first));
}

#[sqlx::test(migrations = "./migrations")]
async fn hidden_poams_are_not_found(pool: PgPool) {
    let fleet = fleet(&pool).await;
    let owner = user(&pool, AuthRole::Operator, &[fleet.env_a, fleet.env_b]).await;
    let token = issue(&pool, owner, WRITE, &[fleet.env_a]).await;
    let (in_scope, in_scope_revision) =
        create_poam(&pool, &fleet.creator, &fleet.observations_a[0]).await;
    let (other, other_revision) =
        create_poam(&pool, &fleet.creator, &fleet.observations_b[0]).await;

    // A plan with one subject in another environment is denied as a whole.
    let (shared, revision) = create_poam(&pool, &fleet.creator, &fleet.observations_a[1]).await;
    let shared_revision = poam_service::link_cve_finding(
        &pool,
        &fleet.creator,
        shared,
        AddCveFindingRequest {
            revision,
            observation: fleet.observations_b[1].clone(),
        },
        &SystemClock,
    )
    .await
    .expect("link subject in B")
    .poam
    .revision;

    for (poam_id, revision) in [
        (other, other_revision),
        (shared, shared_revision),
        (Uuid::new_v4(), 1),
    ] {
        assert!(matches!(
            progress(&pool, &token, poam_id, revision, note()).await,
            Err(ToolError::NotFound)
        ));
    }
    // A wrong revision on a hidden POA&M is still not found, never stale.
    assert!(matches!(
        progress(&pool, &token, shared, shared_revision + 7, note()).await,
        Err(ToolError::NotFound)
    ));

    // An Admin owner gains nothing outside the token's environments.
    let admin = user(&pool, AuthRole::Admin, &[]).await;
    let admin_token = issue(&pool, admin, WRITE, &[fleet.env_a]).await;
    assert!(matches!(
        progress(&pool, &admin_token, other, other_revision, note()).await,
        Err(ToolError::NotFound)
    ));
    let admin_write = progress(&pool, &admin_token, in_scope, in_scope_revision, note())
        .await
        .expect("admin owner writes in A");

    // Removing the owner from the granted environment hides the POA&M.
    clear_user_environment_memberships(&pool, owner)
        .await
        .expect("clear memberships");
    insert_user_environment_membership(&pool, owner, fleet.env_b)
        .await
        .expect("keep only B");
    assert!(matches!(
        progress(&pool, &token, in_scope, revision_of(&admin_write), note()).await,
        Err(ToolError::NotFound)
    ));
    assert_eq!(current_revision(&pool, other).await, other_revision);
    assert_eq!(current_revision(&pool, shared).await, shared_revision);
}

#[sqlx::test(migrations = "./migrations")]
async fn closure_and_stale_revision_are_rejected(pool: PgPool) {
    let fleet = fleet(&pool).await;
    let owner = user(&pool, AuthRole::Operator, &[fleet.env_a]).await;
    let token = issue(&pool, owner, WRITE, &[fleet.env_a]).await;
    let (poam_id, revision) = create_poam(&pool, &fleet.creator, &fleet.observations_a[0]).await;

    assert!(matches!(
        progress(
            &pool,
            &token,
            poam_id,
            revision,
            json!({"type": "set_status", "status": "completed"}),
        )
        .await,
        Err(ToolError::InvalidArguments(message)) if message.starts_with("close_required:")
    ));
    assert!(matches!(
        progress(
            &pool,
            &token,
            poam_id,
            revision,
            json!({"type": "update_poam", "risk": "low"}),
        )
        .await,
        Err(ToolError::InvalidArguments(_))
    ));
    assert!(matches!(
        progress(&pool, &token, poam_id, revision - 1, note()).await,
        Err(ToolError::InvalidArguments(message))
            if message.starts_with("stale_revision:") && message.contains("get_poam_detail")
    ));
    assert_eq!(current_revision(&pool, poam_id).await, revision);
    let status: String = sqlx::query_scalar("SELECT status FROM poams WHERE id=$1")
        .bind(poam_id)
        .fetch_one(&pool)
        .await
        .expect("status");
    assert_eq!(status, "open");
}

// ---------------------------------------------------------------------------
// verify_poam
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "./migrations")]
async fn verify_poam_records_an_attributed_attempt_and_never_closes(pool: PgPool) {
    let fleet = fleet(&pool).await;
    let owner = user(&pool, AuthRole::Operator, &[fleet.env_a]).await;
    let token = issue(&pool, owner, WRITE_VERIFY, &[fleet.env_a]).await;
    let (poam_id, revision) = create_poam(&pool, &fleet.creator, &fleet.observations_a[0]).await;

    // Verification requires awaiting_verification.
    assert!(matches!(
        verify_poam(
            &pool,
            &token.caller,
            json!({"poam_id": poam_id, "revision": revision})
        )
        .await,
        Err(ToolError::InvalidArguments(message)) if message.starts_with("invalid_transition:")
    ));

    let awaiting = progress(
        &pool,
        &token,
        poam_id,
        revision,
        json!({"type": "set_status", "status": "awaiting_verification"}),
    )
    .await
    .expect("await verification");
    let verified = verify_poam(
        &pool,
        &token.caller,
        json!({"poam_id": poam_id, "revision": revision_of(&awaiting)}),
    )
    .await
    .expect("verify");
    assert_eq!(revision_of(&verified), revision_of(&awaiting) + 1);
    // The scan still reports the CVE, so the attempt is rejected.
    assert_eq!(verified["outcome"], "rejected");
    let attempt_id: Uuid =
        serde_json::from_value(verified["attempt_id"].clone()).expect("attempt id");

    let (attempted_by, access_token_id, sealed): (Uuid, Option<Uuid>, bool) = sqlx::query_as(
        "SELECT attempted_by,access_token_id,sealed_at IS NOT NULL \
         FROM poam_verification_attempts WHERE id=$1",
    )
    .bind(attempt_id)
    .fetch_one(&pool)
    .await
    .expect("attempt row");
    assert_eq!(attempted_by, owner);
    assert_eq!(access_token_id, Some(token.caller.token_id));
    assert!(sealed);
    let activity_token: Option<Uuid> = sqlx::query_scalar(
        "SELECT access_token_id FROM poam_activity WHERE poam_id=$1 AND kind='verification_attempted'",
    )
    .bind(poam_id)
    .fetch_one(&pool)
    .await
    .expect("verification activity");
    assert_eq!(activity_token, Some(token.caller.token_id));
    let (status, closed): (String, bool) =
        sqlx::query_as("SELECT status,closed_at IS NOT NULL FROM poams WHERE id=$1")
            .bind(poam_id)
            .fetch_one(&pool)
            .await
            .expect("POA&M state");
    assert_eq!(status, "awaiting_verification");
    assert!(!closed);

    // A POA&M in another environment cannot be verified through this token.
    let (other, other_revision) =
        create_poam(&pool, &fleet.creator, &fleet.observations_b[0]).await;
    assert!(matches!(
        verify_poam(
            &pool,
            &token.caller,
            json!({"poam_id": other, "revision": other_revision})
        )
        .await,
        Err(ToolError::NotFound)
    ));
}

// ---------------------------------------------------------------------------
// No-op milestone updates
// ---------------------------------------------------------------------------

async fn history_counts(pool: &PgPool, poam_id: Uuid) -> (i64, i64) {
    sqlx::query_as(
        "SELECT (SELECT COUNT(*) FROM poam_activity WHERE poam_id=$1), \
                (SELECT COUNT(*) FROM admin_audit_events WHERE target='poam:'||$1::text)",
    )
    .bind(poam_id)
    .fetch_one(pool)
    .await
    .expect("history counts")
}

async fn milestone_updated_at(pool: &PgPool, milestone_id: Uuid) -> chrono::DateTime<Utc> {
    sqlx::query_scalar("SELECT updated_at FROM poam_milestones WHERE id=$1")
        .bind(milestone_id)
        .fetch_one(pool)
        .await
        .expect("milestone updated_at")
}

#[sqlx::test(migrations = "./migrations")]
async fn update_milestone_with_current_values_changes_nothing(pool: PgPool) {
    let fleet = fleet(&pool).await;
    let owner = user(&pool, AuthRole::Operator, &[fleet.env_a]).await;
    let token = issue(&pool, owner, WRITE, &[fleet.env_a]).await;
    let (poam_id, revision) = create_poam(&pool, &fleet.creator, &fleet.observations_a[0]).await;
    let added = progress(
        &pool,
        &token,
        poam_id,
        revision,
        json!({"type": "add_milestone", "title": "Deploy fix", "target_date": "2099-01-31"}),
    )
    .await
    .expect("add milestone");
    let milestone_id: Uuid =
        serde_json::from_value(added["changed"]["milestone_id"].clone()).expect("milestone id");
    let revision = revision_of(&added);
    let history = history_counts(&pool, poam_id).await;
    let updated_at = milestone_updated_at(&pool, milestone_id).await;

    // MCP: every present field already has the requested value (the title
    // after trimming), so the tool reports no change at the same revision.
    let unchanged = progress(
        &pool,
        &token,
        poam_id,
        revision,
        json!({
            "type": "update_milestone",
            "milestone_id": milestone_id,
            "title": "  Deploy fix ",
            "target_date": "2099-01-31",
            "completed": false,
        }),
    )
    .await
    .expect("no-op update");
    assert!(unchanged["changed"].is_null(), "{unchanged}");
    assert_eq!(revision_of(&unchanged), revision);
    assert_eq!(current_revision(&pool, poam_id).await, revision);
    assert_eq!(history_counts(&pool, poam_id).await, history);
    assert_eq!(milestone_updated_at(&pool, milestone_id).await, updated_at);

    // Browser: the same request returns the detail at the same revision.
    let detail = poam_service::update_milestone(
        &pool,
        &fleet.creator,
        poam_id,
        milestone_id,
        UpdateMilestoneRequest {
            revision,
            title: Some("Deploy fix".into()),
            target_date: NaiveDate::from_ymd_opt(2099, 1, 31),
            completed: Some(false),
        },
        &SystemClock,
    )
    .await
    .expect("browser no-op update");
    assert_eq!(detail.poam.revision, revision);
    assert_eq!(history_counts(&pool, poam_id).await, history);
    assert_eq!(milestone_updated_at(&pool, milestone_id).await, updated_at);

    // A real change still records one event and one revision.
    let completed = progress(
        &pool,
        &token,
        poam_id,
        revision,
        json!({"type": "update_milestone", "milestone_id": milestone_id, "completed": true}),
    )
    .await
    .expect("complete milestone");
    assert_eq!(revision_of(&completed), revision + 1);
    assert!(!completed["changed"]["new"]["completed_at"].is_null());
    assert_eq!(
        history_counts(&pool, poam_id).await,
        (history.0 + 1, history.1 + 1)
    );
}

// ---------------------------------------------------------------------------
// Durable attribution
// ---------------------------------------------------------------------------

fn restricted_reader(token: &Token, owner: Uuid) -> PoamActor {
    PoamActor {
        credential: CredentialIdentity::Restricted(token.caller.grant.clone()),
        user_id: owner,
        identifier: String::new(),
        is_admin: false,
        can_mutate: false,
        environment_ids: Vec::new(),
        request_origin: None,
    }
}

#[sqlx::test(migrations = "./migrations")]
async fn agent_attribution_is_stored_on_history_rows(pool: PgPool) {
    let fleet = fleet(&pool).await;
    let owner = user(&pool, AuthRole::Operator, &[fleet.env_a]).await;
    let token = issue(&pool, owner, WRITE_VERIFY, &[fleet.env_a]).await;
    let token_id = token.caller.token_id;
    let (poam_id, revision) = create_poam(&pool, &fleet.creator, &fleet.observations_a[0]).await;
    let awaiting = progress(
        &pool,
        &token,
        poam_id,
        revision,
        json!({"type": "set_status", "status": "awaiting_verification"}),
    )
    .await
    .expect("await verification");
    verify_poam(
        &pool,
        &token.caller,
        json!({"poam_id": poam_id, "revision": revision_of(&awaiting)}),
    )
    .await
    .expect("verify");

    // Agent rows store the marker and the name snapshot; the browser
    // creation keeps the defaults.
    let activity: Vec<(String, bool, Option<String>)> = sqlx::query_as(
        "SELECT kind,via_agent,access_token_name FROM poam_activity \
         WHERE poam_id=$1 ORDER BY created_at,id",
    )
    .bind(poam_id)
    .fetch_all(&pool)
    .await
    .expect("activity rows");
    assert_eq!(activity.len(), 3);
    assert_eq!(activity[0], ("created".to_owned(), false, None));
    for (kind, via_agent, name) in &activity[1..] {
        assert!(*via_agent, "{kind}");
        assert_eq!(name.as_deref(), Some(token.name.as_str()), "{kind}");
    }
    let attempt: (bool, Option<String>) = sqlx::query_as(
        "SELECT via_agent,access_token_name FROM poam_verification_attempts WHERE poam_id=$1",
    )
    .bind(poam_id)
    .fetch_one(&pool)
    .await
    .expect("attempt row");
    assert_eq!(attempt, (true, Some(token.name.clone())));

    // Hard deletion of the token is rejected: the ON DELETE SET NULL action of
    // migration 0341 updates the immutable history rows, and their triggers
    // raise. The token row and every attribution column stay unchanged.
    let error = sqlx::query("DELETE FROM api_access_tokens WHERE id=$1")
        .bind(token_id)
        .execute(&pool)
        .await
        .expect_err("token deletion is rejected while POA&M history names it");
    assert!(
        error.to_string().contains("history is immutable"),
        "unexpected deletion error: {error}"
    );
    let remaining: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM api_access_tokens WHERE id=$1")
        .bind(token_id)
        .fetch_one(&pool)
        .await
        .expect("token count");
    assert_eq!(remaining, 1);

    // The view prefers the snapshot over the live token row: a later rename
    // of the token does not change the recorded name.
    sqlx::query("UPDATE api_access_tokens SET name='renamed-after-write' WHERE id=$1")
        .bind(token_id)
        .execute(&pool)
        .await
        .expect("rename token");
    let detail = poam_service::restricted_detail(
        &pool,
        &restricted_reader(&token, owner),
        poam_id,
        &SystemClock,
    )
    .await
    .expect("detail");
    let status_view = detail
        .activity
        .iter()
        .find(|item| item.kind == "status_changed")
        .expect("status activity");
    assert!(status_view.via_agent);
    assert_eq!(status_view.access_token_id, Some(token_id));
    assert_eq!(
        status_view.access_token_name.as_deref(),
        Some(token.name.as_str())
    );
    let created_view = detail
        .activity
        .iter()
        .find(|item| item.kind == "created")
        .expect("created activity");
    assert!(!created_view.via_agent);
    assert_eq!(created_view.access_token_name, None);
    let attempt_view = detail
        .verification_attempts
        .first()
        .expect("verification attempt");
    assert!(attempt_view.via_agent);
    assert_eq!(
        attempt_view.access_token_name.as_deref(),
        Some(token.name.as_str())
    );
}

// ---------------------------------------------------------------------------
// Concurrent scope changes
// ---------------------------------------------------------------------------

/// Bounds each race so that a locking defect fails the test instead of
/// hanging it.
const RACE_TIMEOUT: Duration = Duration::from_secs(30);

// The exact statements of the service locks; `pg_stat_activity.query` shows
// the waiting statement text.
const ENVIRONMENT_LOCK_SQL: &str =
    "SELECT id FROM environments WHERE id=ANY($1) ORDER BY id FOR SHARE";
const SYSTEM_LOCK_SQL: &str = "SELECT id FROM systems WHERE id=ANY($1) ORDER BY id FOR SHARE";
const POAM_LOCK_SQL: &str = "SELECT revision,status FROM poams WHERE id=$1 FOR UPDATE";

/// Polls for up to five seconds for a backend that waits for a lock while it
/// runs `statement`.
async fn lock_waiter_seen(pool: &PgPool, statement: &str) -> bool {
    for _ in 0..200 {
        let waiting: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM pg_stat_activity \
             WHERE datname=current_database() AND wait_event_type='Lock' AND query=$1)",
        )
        .bind(statement)
        .fetch_one(pool)
        .await
        .expect("pg_stat_activity");
        if waiting {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    false
}

/// Runs `statement` with a 300 ms lock timeout in a transaction that always
/// rolls back, and reports whether a lock blocked it (SQLSTATE 55P03).
async fn blocked_by_lock(pool: &PgPool, statement: &str) -> bool {
    let mut tx = pool.begin().await.expect("begin probe");
    sqlx::query("SET LOCAL lock_timeout = '300ms'")
        .execute(&mut *tx)
        .await
        .expect("set lock_timeout");
    let result = sqlx::query(statement).execute(&mut *tx).await;
    tx.rollback().await.expect("roll back probe");
    match result {
        Ok(_) => false,
        Err(error) => {
            let code = error
                .as_database_error()
                .and_then(|database| database.code())
                .map(|code| code.into_owned());
            assert_eq!(
                code.as_deref(),
                Some("55P03"),
                "unexpected probe error: {error}"
            );
            true
        }
    }
}

#[sqlx::test(migrations = "./migrations")]
async fn environment_deactivation_committed_during_the_lock_wait_is_observed(pool: PgPool) {
    let fleet = fleet(&pool).await;
    let owner = user(&pool, AuthRole::Operator, &[fleet.env_a]).await;
    let token = issue(&pool, owner, WRITE, &[fleet.env_a]).await;
    let (poam_id, revision) = create_poam(&pool, &fleet.creator, &fleet.observations_a[0]).await;
    let history = history_counts(&pool, poam_id).await;

    let mut deactivation = pool.begin().await.expect("begin deactivation");
    sqlx::query("UPDATE environments SET is_active=false WHERE id=$1")
        .bind(fleet.env_a)
        .execute(&mut *deactivation)
        .await
        .expect("deactivate environment");
    let release = async {
        let waited = lock_waiter_seen(&pool, ENVIRONMENT_LOCK_SQL).await;
        deactivation.commit().await.expect("commit deactivation");
        waited
    };
    let (result, waited) = tokio::time::timeout(
        RACE_TIMEOUT,
        futures::future::join(progress(&pool, &token, poam_id, revision, note()), release),
    )
    .await
    .expect("race finishes");

    assert!(waited, "the write must wait for the environment row lock");
    assert!(matches!(result, Err(ToolError::NotFound)), "{result:?}");
    assert_eq!(current_revision(&pool, poam_id).await, revision);
    assert_eq!(history_counts(&pool, poam_id).await, history);
}

#[sqlx::test(migrations = "./migrations")]
async fn subject_system_move_committed_during_the_lock_wait_is_observed(pool: PgPool) {
    let fleet = fleet(&pool).await;
    let owner = user(&pool, AuthRole::Operator, &[fleet.env_a]).await;
    let token = issue(&pool, owner, WRITE, &[fleet.env_a]).await;
    let (poam_id, revision) = create_poam(&pool, &fleet.creator, &fleet.observations_a[0]).await;
    let history = history_counts(&pool, poam_id).await;

    let mut move_system = pool.begin().await.expect("begin move");
    sqlx::query("UPDATE systems SET environment_id=$2 WHERE id=$1")
        .bind(fleet.observations_a[0].system_id)
        .bind(fleet.env_b)
        .execute(&mut *move_system)
        .await
        .expect("move system to B");
    let release = async {
        let waited = lock_waiter_seen(&pool, SYSTEM_LOCK_SQL).await;
        move_system.commit().await.expect("commit move");
        waited
    };
    let (result, waited) = tokio::time::timeout(
        RACE_TIMEOUT,
        futures::future::join(progress(&pool, &token, poam_id, revision, note()), release),
    )
    .await
    .expect("race finishes");

    assert!(
        waited,
        "the write must wait for the subject system row lock"
    );
    assert!(matches!(result, Err(ToolError::NotFound)), "{result:?}");
    assert_eq!(current_revision(&pool, poam_id).await, revision);
    assert_eq!(history_counts(&pool, poam_id).await, history);
}

#[sqlx::test(migrations = "./migrations")]
async fn a_write_in_progress_blocks_deactivation_and_moves_until_it_commits(pool: PgPool) {
    let fleet = fleet(&pool).await;
    let owner = user(&pool, AuthRole::Operator, &[fleet.env_a]).await;
    let token = issue(&pool, owner, WRITE, &[fleet.env_a]).await;
    let (poam_id, revision) = create_poam(&pool, &fleet.creator, &fleet.observations_a[0]).await;
    let deactivate = format!(
        "UPDATE environments SET is_active=false WHERE id='{}'",
        fleet.env_a
    );
    let move_system = format!(
        "UPDATE systems SET environment_id='{}' WHERE id='{}'",
        fleet.env_b, fleet.observations_a[0].system_id
    );

    // Holding the POA&M row stops the write at its last lock (step 6), after
    // it has taken the environment and system locks.
    let mut holder = pool.begin().await.expect("begin holder");
    sqlx::query("SELECT id FROM poams WHERE id=$1 FOR UPDATE")
        .bind(poam_id)
        .execute(&mut *holder)
        .await
        .expect("hold POA&M row");
    let probe = async {
        let waited = lock_waiter_seen(&pool, POAM_LOCK_SQL).await;
        let deactivation_blocked = blocked_by_lock(&pool, &deactivate).await;
        let move_blocked = blocked_by_lock(&pool, &move_system).await;
        holder.rollback().await.expect("release POA&M row");
        (waited, deactivation_blocked, move_blocked)
    };
    let (result, (waited, deactivation_blocked, move_blocked)) = tokio::time::timeout(
        RACE_TIMEOUT,
        futures::future::join(progress(&pool, &token, poam_id, revision, note()), probe),
    )
    .await
    .expect("race finishes");

    assert!(waited, "the write must reach the POA&M row lock");
    assert!(deactivation_blocked, "deactivation must wait for the write");
    assert!(move_blocked, "a subject move must wait for the write");
    let written = result.expect("the write commits after the POA&M row is released");
    assert_eq!(revision_of(&written), revision + 1);
    // The committed write releases its scope locks.
    assert!(!blocked_by_lock(&pool, &deactivate).await);
    assert!(!blocked_by_lock(&pool, &move_system).await);
}

// ---------------------------------------------------------------------------
// Verification against the real system move
// ---------------------------------------------------------------------------

/// The first lock statement of `verify_once` (`lock_cve_keys_tx`), byte for
/// byte, for `lock_waiter_seen`.
const VERIFY_CVE_KEY_LOCK_SQL: &str = "SELECT lock_poam_cve_key(key.cve_id)\n           FROM (SELECT DISTINCT unnest($1::text[]) AS cve_id ORDER BY cve_id) key";

const MOVE_CSRF: &str = "mcp-write-move-csrf";

/// The real `PATCH /api/v1/systems/:id` handler (`update_system_handler`),
/// mounted on the path of `bin/server.rs`, with an Admin browser session.
struct MoveApi {
    base: String,
    client: Client,
    session: String,
}

async fn move_api(pool: &PgPool, admin: Uuid) -> MoveApi {
    let session = format!("mcp-write-move-session-{}", Uuid::new_v4().simple());
    create_user_session(
        pool,
        admin,
        hash_token(&session),
        Utc::now() + chrono::Duration::hours(1),
        Some("mcp-write-poam-test".into()),
        Some("127.0.0.1".into()),
        "local".into(),
    )
    .await
    .expect("create Admin session");
    let state = CFState::new(
        pool.clone(),
        ServerConfig::default(),
        Arc::new(QueueNotifier::new()),
        BackgroundJobRegistry::new(),
    );
    let app = Router::new()
        .route("/api/v1/systems/:id", patch(systems::update_system_handler))
        .with_state(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind system move server");
    let address = listener.local_addr().expect("read system move address");
    tokio::spawn(async move {
        axum::serve(listener, app)
            .await
            .expect("serve system move API")
    });
    MoveApi {
        base: format!("http://{address}"),
        client: Client::new(),
        session,
    }
}

impl MoveApi {
    /// Sends one system edit with a matching double-submit CSRF token and
    /// returns the HTTP status and body.
    async fn move_system(&self, system_id: Uuid, body: &Value) -> (u16, String) {
        let response = self
            .client
            .patch(format!("{}/api/v1/systems/{system_id}", self.base))
            .header(
                "cookie",
                format!(
                    "{SESSION_COOKIE_NAME}={}; {CSRF_COOKIE_NAME}={MOVE_CSRF}",
                    self.session
                ),
            )
            .header(CSRF_HEADER_NAME.as_str(), MOVE_CSRF)
            .json(body)
            .send()
            .await
            .expect("send system move");
        let status = response.status().as_u16();
        let text = response.text().await.expect("read system move response");
        (status, text)
    }
}

/// Returns the edit body that keeps every current value of the system and
/// changes only its environment.
async fn move_body(pool: &PgPool, system_id: Uuid, environment_id: Uuid) -> Value {
    let (hostname, configuration, flake, environment): (String, Option<String>, String, String) =
        sqlx::query_as(
            "SELECT s.hostname,s.system_configuration_name,f.name,e.name \
             FROM systems s JOIN flakes f ON f.id=s.flake_id \
             JOIN environments e ON e.id=$2 WHERE s.id=$1",
        )
        .bind(system_id)
        .bind(environment_id)
        .fetch_one(pool)
        .await
        .expect("system move body");
    json!({
        "hostname": hostname,
        "system_configuration_name": configuration,
        "flake_name": flake,
        "environment": environment,
        "deployment_policy": "manual",
    })
}

async fn system_environment(pool: &PgPool, system_id: Uuid) -> Option<Uuid> {
    sqlx::query_scalar("SELECT environment_id FROM systems WHERE id=$1")
        .bind(system_id)
        .fetch_one(pool)
        .await
        .expect("system environment")
}

async fn attempt_count(pool: &PgPool, poam_id: Uuid) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM poam_verification_attempts WHERE poam_id=$1")
        .bind(poam_id)
        .fetch_one(pool)
        .await
        .expect("attempt count")
}

#[sqlx::test(migrations = "./migrations")]
async fn verification_serializes_with_a_real_system_move(pool: PgPool) {
    let fleet = fleet(&pool).await;
    let owner = user(&pool, AuthRole::Operator, &[fleet.env_a]).await;
    let token = issue(&pool, owner, WRITE_VERIFY, &[fleet.env_a]).await;
    let system_id = fleet.observations_a[0].system_id;
    let (poam_id, revision) = create_poam(&pool, &fleet.creator, &fleet.observations_a[0]).await;
    progress(
        &pool,
        &token,
        poam_id,
        revision,
        json!({"type": "set_status", "status": "awaiting_verification"}),
    )
    .await
    .expect("await verification");
    let api = move_api(&pool, fleet.creator.user_id).await;
    let to_b = move_body(&pool, system_id, fleet.env_b).await;
    let to_a = move_body(&pool, system_id, fleet.env_a).await;

    // Scenario a: verification races the real move handler (A to B), and the
    // system then moves back to A for the next iteration.
    //
    // CONCURRENCY: Both transactions take the exclusive advisory key of
    // CVE-2099-0701 first (`lock_cve_keys_tx` and the move's
    // `lock_poam_cve_key`) and hold it until commit, so they serialize
    // completely. There are only two coherent results:
    // - verification held the key first: it records an attempt while the
    //   system is still in A, and the move commits after it;
    // - the move held the key first: verification observes the system in B
    //   and returns `NotFound` without an attempt.
    // Any other result, a deadlock (40P01 surfaced as an error or a 500 from
    // the move, which does not retry it) or a timeout fails the test. The join
    // order alternates so both sides get the first poll.
    for iteration in 0..20 {
        let revision = current_revision(&pool, poam_id).await;
        let attempts = attempt_count(&pool, poam_id).await;
        let verify = verify_poam(
            &pool,
            &token.caller,
            json!({"poam_id": poam_id, "revision": revision}),
        );
        let moving = api.move_system(system_id, &to_b);
        let (verified, moved) = if iteration % 2 == 0 {
            tokio::time::timeout(RACE_TIMEOUT, futures::future::join(verify, moving))
                .await
                .expect("race finishes")
        } else {
            let (moved, verified) =
                tokio::time::timeout(RACE_TIMEOUT, futures::future::join(moving, verify))
                    .await
                    .expect("race finishes");
            (verified, moved)
        };

        assert_eq!(moved.0, 200, "iteration {iteration}: move: {}", moved.1);
        assert_eq!(
            system_environment(&pool, system_id).await,
            Some(fleet.env_b),
            "iteration {iteration}"
        );
        match verified {
            Ok(result) => {
                assert_eq!(revision_of(&result), revision + 1, "iteration {iteration}");
                assert_eq!(result["outcome"], "rejected", "iteration {iteration}");
                let attempt_id: Uuid =
                    serde_json::from_value(result["attempt_id"].clone()).expect("attempt id");
                let (poam_revision, sealed): (i64, bool) = sqlx::query_as(
                    "SELECT poam_revision,sealed_at IS NOT NULL \
                     FROM poam_verification_attempts WHERE id=$1 AND poam_id=$2",
                )
                .bind(attempt_id)
                .bind(poam_id)
                .fetch_one(&pool)
                .await
                .expect("attempt row");
                assert_eq!(poam_revision, revision, "iteration {iteration}");
                assert!(sealed, "iteration {iteration}");
                assert_eq!(attempt_count(&pool, poam_id).await, attempts + 1);
                assert_eq!(current_revision(&pool, poam_id).await, revision + 1);
            }
            Err(ToolError::NotFound) => {
                assert_eq!(attempt_count(&pool, poam_id).await, attempts);
                assert_eq!(current_revision(&pool, poam_id).await, revision);
            }
            Err(error) => panic!("iteration {iteration}: unexpected verification: {error:?}"),
        }

        let back = tokio::time::timeout(RACE_TIMEOUT, api.move_system(system_id, &to_a))
            .await
            .expect("move back finishes");
        assert_eq!(back.0, 200, "iteration {iteration}: move back: {}", back.1);
        assert_eq!(
            system_environment(&pool, system_id).await,
            Some(fleet.env_a)
        );
    }

    // Scenario b: a move transaction runs the statements of
    // `update_system_handler` in its order. It holds its first locks (the
    // CVE keys, then source and destination environments `FOR UPDATE`) while
    // verification starts. Verification waits at its first lock, the CVE key.
    // The move then takes the rest of its locks, changes the environment and
    // commits. Under READ COMMITTED the verification statements after the
    // wait see the system in B, so verification is `NotFound` and records
    // nothing.
    let revision = current_revision(&pool, poam_id).await;
    let attempts = attempt_count(&pool, poam_id).await;
    let mut move_tx = pool.begin().await.expect("begin move");
    let preliminary: Option<Uuid> =
        sqlx::query_scalar("SELECT environment_id FROM systems WHERE id=$1")
            .bind(system_id)
            .fetch_one(&mut *move_tx)
            .await
            .expect("preliminary environment");
    assert_eq!(preliminary, Some(fleet.env_a));
    let environment_b_name = to_b["environment"].as_str().expect("environment name");
    let cve_keys: Vec<String> = sqlx::query_scalar(
        r#"SELECT DISTINCT canonical_cve_id FROM (
             SELECT canonical_cve_id FROM poam_cve_findings
             WHERE system_id=$1
             UNION ALL
             SELECT disposition.canonical_cve_id
             FROM cve_environment_dispositions disposition
             WHERE disposition.state='scheduled'
               AND disposition.retired_at IS NULL
               AND (disposition.environment_id=(SELECT environment_id
                    FROM systems WHERE id=$1)
                 OR disposition.environment_id=(SELECT id FROM environments
                    WHERE name=$2))
           ) keys ORDER BY canonical_cve_id"#,
    )
    .bind(system_id)
    .bind(environment_b_name)
    .fetch_all(&mut *move_tx)
    .await
    .expect("move CVE keys");
    assert!(cve_keys.iter().any(|key| key == "CVE-2099-0701"));
    sqlx::query(
        r#"SELECT lock_poam_cve_key(key.cve_id)
           FROM (SELECT unnest($1::text[]) AS cve_id ORDER BY cve_id) key"#,
    )
    .bind(&cve_keys)
    .execute(&mut *move_tx)
    .await
    .expect("lock move CVE keys");
    let mut environment_ids = vec![fleet.env_a, fleet.env_b];
    environment_ids.sort();
    sqlx::query("SELECT id FROM environments WHERE id=ANY($1) ORDER BY id FOR UPDATE")
        .bind(&environment_ids)
        .execute(&mut *move_tx)
        .await
        .expect("lock move environments");

    let release = async {
        let waited = lock_waiter_seen(&pool, VERIFY_CVE_KEY_LOCK_SQL).await;
        sqlx::query("SELECT lock_poam_finding_key($1,$2)")
            .bind(system_id)
            .bind(Uuid::nil())
            .execute(&mut *move_tx)
            .await
            .expect("lock move system key");
        sqlx::query(
            r#"SELECT lock_poam_finding_key(key.system_id,key.policy_lineage_id)
               FROM (
                 SELECT finding.system_id,finding.policy_lineage_id
                 FROM poam_findings finding WHERE finding.system_id=$1
                 ORDER BY finding.system_id,finding.policy_lineage_id
               ) key"#,
        )
        .bind(system_id)
        .execute(&mut *move_tx)
        .await
        .expect("lock move policy finding keys");
        sqlx::query(
            r#"SELECT lock_poam_cve_finding_key(
                        key.system_id,key.canonical_cve_id,key.canonical_package_name)
               FROM (
                 SELECT finding.system_id,finding.canonical_cve_id,
                        finding.canonical_package_name
                 FROM poam_cve_findings finding WHERE finding.system_id=$1
                 ORDER BY finding.system_id,finding.canonical_cve_id,
                          finding.canonical_package_name
               ) key"#,
        )
        .bind(system_id)
        .execute(&mut *move_tx)
        .await
        .expect("lock move CVE finding keys");
        sqlx::query(
            "SELECT environment_id, management_mode, parent_host_id, is_active
             FROM systems WHERE id=$1 FOR UPDATE",
        )
        .bind(system_id)
        .execute(&mut *move_tx)
        .await
        .expect("lock moved system");
        // `update_system_metadata` with unchanged FQDN and heartbeat; every
        // column except the environment keeps its value.
        sqlx::query(
            "UPDATE systems
             SET hostname = hostname,
                 environment_id = $1,
                 flake_id = flake_id,
                 system_configuration_name = system_configuration_name,
                 deployment_policy = deployment_policy,
                 updated_at = NOW()
              WHERE id = $2",
        )
        .bind(fleet.env_b)
        .bind(system_id)
        .execute(&mut *move_tx)
        .await
        .expect("move system to B");
        move_tx.commit().await.expect("commit move");
        waited
    };
    let (result, waited) = tokio::time::timeout(
        RACE_TIMEOUT,
        futures::future::join(
            verify_poam(
                &pool,
                &token.caller,
                json!({"poam_id": poam_id, "revision": revision}),
            ),
            release,
        ),
    )
    .await
    .expect("race finishes");

    assert!(waited, "verification must wait for the move's first lock");
    assert!(matches!(result, Err(ToolError::NotFound)), "{result:?}");
    assert_eq!(
        system_environment(&pool, system_id).await,
        Some(fleet.env_b)
    );
    assert_eq!(attempt_count(&pool, poam_id).await, attempts);
    assert_eq!(current_revision(&pool, poam_id).await, revision);
}
