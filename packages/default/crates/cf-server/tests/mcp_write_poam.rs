//! POA&M write MCP tool regressions (TASK-534.5).
//!
//! Each SQLx test owns an isolated database with the full production
//! migrations. Tokens are issued through the access-token service and admitted
//! with `validate_presented_token`, as the MCP transport does, so the in-
//! transaction token-row check sees real rows. Tests call the tool functions
//! directly.

use chrono::Utc;
use crystal_forge::api::models::CreateAccessTokenRequest;
use crystal_forge::auth::authorization::CredentialIdentity;
use crystal_forge::mcp::poam_write::{update_poam_progress, verify_poam};
use crystal_forge::mcp::tools::{ToolCaller, ToolError};
use crystal_forge::models::auth_identity::AuthRole;
use crystal_forge::models::poam::{
    AddCveFindingRequest, AddNoteRequest, CreateCvePoamRequest, CveObservationReference, PoamRisk,
};
use crystal_forge::queries::auth_identity::{
    clear_user_environment_memberships, insert_user_environment_membership, sync_user_role,
};
use crystal_forge::queries::users::insert_user;
use crystal_forge::services::access_tokens::{
    self as token_service, AccessTokenSettings, validate_presented_token,
};
use crystal_forge::services::poam::{self as poam_service, PoamActor, SystemClock};
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
