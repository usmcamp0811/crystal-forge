//! Read-only MCP tool authorization regressions (TASK-534.4).
//!
//! Each SQLx test owns an isolated database with the full production
//! migrations. Tests call `call_tool` directly. [`caller`] reads the owner's
//! role and memberships the way `validate_presented_token` does at the start
//! of each MCP request, so rebuilding it models a new request.

use chrono::{Duration, Utc};
use crystal_forge::api::models::SystemCveInventoryParams;
use crystal_forge::auth::authorization::{CredentialIdentity, RestrictedGrant};
use crystal_forge::mcp::tools::{ToolCaller, ToolError, call_tool, visible_tools};
use crystal_forge::models::auth_identity::AuthRole;
use crystal_forge::models::poam::{
    AddCveFindingRequest, CreateCvePoamRequest, CveObservationReference, PoamRisk,
};
use crystal_forge::queries::auth_identity::{
    clear_user_environment_memberships, insert_user_environment_membership, sync_user_role,
};
use crystal_forge::queries::cves::{
    SystemCveInventoryPageRequest, fetch_system_cve_inventory_page,
};
use crystal_forge::queries::users::insert_user;
use crystal_forge::services::poam::{self as poam_service, PoamActor, SystemClock};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

const CVES: &[&str] = &["cves:read"];
const CVES_POAMS: &[&str] = &["cves:read", "poams:read"];
const CVES_PROVENANCE: &[&str] = &["cves:read", "provenance:read"];
const POAMS: &[&str] = &["poams:read"];
/// Fake credential embedded in every fixture flake URL. No tool output may
/// contain it.
const REPO_SECRET: &str = "mcp-fixture-secret";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/// One managed system with a consistent running report.
struct Host {
    system_id: Uuid,
    derivation_id: i32,
    commit_hash: String,
    store_path: String,
}

/// One finding to seal into an exact scan.
struct Finding<'a> {
    cve_id: &'a str,
    package: &'a str,
    cvss: Option<f64>,
    fixed_version: Option<&'a str>,
}

fn finding<'a>(cve_id: &'a str, package: &'a str) -> Finding<'a> {
    Finding {
        cve_id,
        package,
        cvss: Some(5.0),
        fixed_version: None,
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
        &format!("mcp-{suffix}@example.invalid"),
        Some("MCP Test"),
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

/// Builds a caller for one request from the owner's current database state.
async fn caller(pool: &PgPool, owner: Uuid, scopes: &[&str], grant: &[Uuid]) -> ToolCaller {
    let owner_is_admin: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM user_role_assignments WHERE user_id=$1 AND role='admin')",
    )
    .bind(owner)
    .fetch_one(pool)
    .await
    .expect("owner role");
    let owner_environment_ids: Vec<Uuid> = sqlx::query_scalar(
        "SELECT environment_id FROM user_environment_memberships WHERE user_id=$1 ORDER BY environment_id",
    )
    .bind(owner)
    .fetch_all(pool)
    .await
    .expect("owner memberships");
    let token_id = Uuid::new_v4();
    ToolCaller {
        token_id,
        owner_user_id: owner,
        owner_is_admin,
        owner_environment_ids,
        grant: RestrictedGrant::new(
            token_id,
            Some("mcp".into()),
            scopes.iter().map(|scope| (*scope).to_owned()).collect(),
            grant.iter().copied().collect(),
            Utc::now() + Duration::hours(1),
        ),
    }
}

/// Creates a managed system. With `report`, it also records a consistent
/// running report for its NixOS derivation, which establishes Current CVE
/// authority once a schema-1 scan is sealed.
async fn host(pool: &PgPool, environment_id: Uuid, report: bool) -> Host {
    let suffix = Uuid::new_v4().simple().to_string();
    let hostname = format!("mcp-{suffix}");
    let flake: i32 =
        sqlx::query_scalar("INSERT INTO flakes(name,repo_url) VALUES($1,$2) RETURNING id")
            .bind(format!("mcp-flake-{suffix}"))
            .bind(format!(
                "https://mcp-user:{REPO_SECRET}@example.invalid/{suffix}.git"
            ))
            .fetch_one(pool)
            .await
            .expect("insert flake");
    let commit_hash = format!("{suffix}{}", &suffix[..8]);
    let commit: i32 = sqlx::query_scalar(
        "INSERT INTO commits(flake_id,git_commit_hash,commit_timestamp) VALUES($1,$2,now()) RETURNING id",
    )
    .bind(flake)
    .bind(&commit_hash)
    .fetch_one(pool)
    .await
    .expect("insert commit");
    let system_id: Uuid = sqlx::query_scalar(
        "INSERT INTO systems(hostname,is_active,public_key,derivation,flake_id,system_configuration_name,environment_id) \
         VALUES($1,true,$2,'test-derivation',$3,$1,$4) RETURNING id",
    )
    .bind(&hostname)
    .bind(format!("mcp-key-{suffix}"))
    .bind(flake)
    .bind(environment_id)
    .fetch_one(pool)
    .await
    .expect("insert system");
    let store_path = format!("/nix/store/{suffix}-mcp-system");
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
    if report {
        sqlx::query(
            "INSERT INTO system_states(hostname,change_reason,store_path,generation, \
             generation_matches_current_store_path) VALUES($1,'startup',$2,1,true)",
        )
        .bind(&hostname)
        .bind(&store_path)
        .execute(pool)
        .await
        .expect("insert running report");
    }
    Host {
        system_id,
        derivation_id,
        commit_hash,
        store_path,
    }
}

/// Seals one completed schema-1 scan of the host's derivation.
async fn seal_scan(
    pool: &PgPool,
    host: &Host,
    findings: &[Finding<'_>],
) -> Vec<CveObservationReference> {
    let scan_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO cve_scans(id,derivation_id,scanner_name,status) VALUES($1,$2,'mcp-test','in_progress')",
    )
    .bind(scan_id)
    .bind(host.derivation_id)
    .execute(pool)
    .await
    .expect("insert scan");
    let mut references = Vec::new();
    for item in findings {
        sqlx::query(
            "INSERT INTO cves(id,cvss_v3_score,description) VALUES($1,CAST($2 AS numeric),$3) \
             ON CONFLICT(id) DO NOTHING",
        )
        .bind(item.cve_id)
        .bind(item.cvss)
        .bind("Parser overflow\u{1b}[2J allows code execution")
        .execute(pool)
        .await
        .expect("insert CVE");
        let suffix = Uuid::new_v4().simple().to_string();
        let path = format!("/nix/store/{suffix}-{}.drv", item.package);
        if let Some(fixed_version) = item.fixed_version {
            let package_derivation: i32 = sqlx::query_scalar(
                "INSERT INTO derivations(commit_id,derivation_type,derivation_name,derivation_path, \
                 pname,version,status_id,attempt_count) \
                 VALUES(NULL,'package',$1,$2,$3,'1.0',11,0) RETURNING id",
            )
            .bind(format!("{}-1.0-{suffix}", item.package))
            .bind(&path)
            .bind(item.package)
            .fetch_one(pool)
            .await
            .expect("insert package derivation");
            sqlx::query(
                "INSERT INTO package_vulnerabilities(derivation_id,cve_id,is_whitelisted,fixed_version) \
                 VALUES($1,$2,false,$3)",
            )
            .bind(package_derivation)
            .bind(item.cve_id)
            .bind(fixed_version)
            .execute(pool)
            .await
            .expect("insert fixed version");
        }
        sqlx::query(
            "INSERT INTO cve_scan_vulnerability_observations( \
             scan_id,canonical_cve_id,canonical_package_name,observed_package_name, \
             observed_package_version,observed_derivation_path,is_whitelisted,detection_method) \
             VALUES($1,$2,$3,$3,'1.0',$4,false,'mcp-test')",
        )
        .bind(scan_id)
        .bind(item.cve_id)
        .bind(item.package)
        .bind(&path)
        .execute(pool)
        .await
        .expect("insert observation");
        references.push(CveObservationReference {
            system_id: host.system_id,
            scan_id,
            occurrence_derivation_path: path,
            canonical_cve_id: item.cve_id.into(),
            canonical_package_name: item.package.into(),
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

/// Two environments, each with one scanned host carrying the same findings.
struct Fleet {
    env_a: Uuid,
    env_b: Uuid,
    host_a: Host,
    host_b: Host,
    observations_a: Vec<CveObservationReference>,
    observations_b: Vec<CveObservationReference>,
}

async fn fleet(pool: &PgPool) -> Fleet {
    let env_a = environment(pool, "mcp-a").await;
    let env_b = environment(pool, "mcp-b").await;
    let host_a = host(pool, env_a, true).await;
    let host_b = host(pool, env_b, true).await;
    let findings = [
        Finding {
            cve_id: "CVE-2099-0001",
            package: "pkg",
            cvss: Some(9.8),
            fixed_version: Some("1.1"),
        },
        finding("CVE-2099-0002", "pkg2"),
    ];
    let observations_a = seal_scan(pool, &host_a, &findings).await;
    let observations_b = seal_scan(pool, &host_b, &findings).await;
    Fleet {
        env_a,
        env_b,
        host_a,
        host_b,
        observations_a,
        observations_b,
    }
}

async fn call(
    pool: &PgPool,
    caller: &ToolCaller,
    tool: &str,
    arguments: Value,
) -> Result<Value, ToolError> {
    call_tool(pool, caller, tool, arguments).await
}

fn system_ids(result: &Value) -> Vec<String> {
    result["items"]
        .as_array()
        .expect("items array")
        .iter()
        .map(|item| item["system_id"].as_str().expect("system id").to_owned())
        .collect()
}

fn browser_admin(user_id: Uuid) -> PoamActor {
    PoamActor {
        credential: CredentialIdentity::BrowserSession,
        user_id,
        identifier: format!("mcp-admin-{user_id}@example.invalid"),
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

// ---------------------------------------------------------------------------
// Discovery and scopes
// ---------------------------------------------------------------------------

#[test]
fn visible_tools_follow_granted_scopes() {
    let names = |scopes: &[&str]| {
        let caller = ToolCaller {
            token_id: Uuid::new_v4(),
            owner_user_id: Uuid::new_v4(),
            owner_is_admin: false,
            owner_environment_ids: Vec::new(),
            grant: RestrictedGrant::new(
                Uuid::new_v4(),
                Some("mcp".into()),
                scopes.iter().map(|scope| (*scope).to_owned()).collect(),
                [Uuid::new_v4()].into(),
                Utc::now() + Duration::hours(1),
            ),
        };
        visible_tools(&caller)
            .into_iter()
            .map(|tool| tool.name)
            .collect::<Vec<_>>()
    };
    assert_eq!(
        names(CVES),
        [
            "find_systems",
            "list_scan_findings",
            "list_remediation_work",
            "get_system_security_context",
            "get_scan_result"
        ]
    );
    assert_eq!(
        names(CVES_PROVENANCE),
        [
            "find_systems",
            "list_scan_findings",
            "list_remediation_work",
            "get_system_security_context",
            "get_remediation_context",
            "get_scan_result"
        ]
    );
    assert_eq!(names(POAMS), ["get_poam_detail"]);
    // Write tools appear only with their own write scope.
    assert_eq!(names(&["scans:request"]), ["request_cve_scan"]);
    assert_eq!(names(&["poams:create"]), ["create_cve_poam"]);
    assert_eq!(names(&["poams:write"]), ["update_poam_progress"]);
    assert_eq!(names(&["poams:verify"]), ["verify_poam"]);
    assert!(names(&[]).is_empty());
}

#[sqlx::test(migrations = "./migrations")]
async fn missing_scope_is_forbidden_and_unknown_tool_is_rejected(pool: PgPool) {
    let env = environment(&pool, "mcp-scope").await;
    let owner = user(&pool, AuthRole::Operator, &[env]).await;
    let poams_only = caller(&pool, owner, POAMS, &[env]).await;
    assert!(matches!(
        call(&pool, &poams_only, "list_remediation_work", json!({})).await,
        Err(ToolError::Forbidden(_))
    ));
    let cves_only = caller(&pool, owner, CVES, &[env]).await;
    assert!(matches!(
        call(
            &pool,
            &cves_only,
            "get_poam_detail",
            json!({"poam_id": Uuid::new_v4()})
        )
        .await,
        Err(ToolError::Forbidden(_))
    ));
    assert!(matches!(
        call(
            &pool,
            &cves_only,
            "list_remediation_work",
            json!({"include_poams": true})
        )
        .await,
        Err(ToolError::Forbidden(_))
    ));
    assert!(matches!(
        call(&pool, &cves_only, "run_shell", json!({})).await,
        Err(ToolError::UnknownTool)
    ));
    assert!(matches!(
        call(
            &pool,
            &cves_only,
            "list_remediation_work",
            json!({"verbose": true})
        )
        .await,
        Err(ToolError::InvalidArguments(_))
    ));
    assert!(matches!(
        call(
            &pool,
            &cves_only,
            "list_remediation_work",
            json!({"limit": 101})
        )
        .await,
        Err(ToolError::InvalidArguments(_))
    ));
}

// ---------------------------------------------------------------------------
// list_remediation_work
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "./migrations")]
async fn list_work_is_limited_to_grant_owner_and_active_environments(pool: PgPool) {
    let fleet = fleet(&pool).await;
    let host_a = fleet.host_a.system_id.to_string();
    let owner = user(&pool, AuthRole::Operator, &[fleet.env_a, fleet.env_b]).await;

    let token = caller(&pool, owner, CVES, &[fleet.env_a]).await;
    let result = call(&pool, &token, "list_remediation_work", json!({}))
        .await
        .expect("list work");
    assert_eq!(system_ids(&result), [host_a.clone(), host_a.clone()]);
    let item = &result["items"][0];
    assert_eq!(item["cve_id"], "CVE-2099-0001");
    assert_eq!(item["severity"], "critical");
    assert_eq!(item["fixed_version"], "1.1");
    assert_eq!(item["evidence_authority"], "exact_current_scan");
    assert_eq!(item["environment_id"], json!(fleet.env_a));
    assert!(item.get("poam").is_none());
    assert!(result.get("total").is_none());

    // An explicit selector never widens the grant.
    assert!(matches!(
        call(&pool, &token, "list_remediation_work", json!({"environment_ids": [fleet.env_b]})).await,
        Err(ToolError::InvalidArguments(message)) if message.contains(&fleet.env_b.to_string())
    ));

    // Removing the owner from the granted environment removes the rows.
    clear_user_environment_memberships(&pool, owner)
        .await
        .expect("clear memberships");
    insert_user_environment_membership(&pool, owner, fleet.env_b)
        .await
        .expect("keep only B");
    let token = caller(&pool, owner, CVES, &[fleet.env_a]).await;
    let result = call(&pool, &token, "list_remediation_work", json!({}))
        .await
        .expect("list after removal");
    assert!(system_ids(&result).is_empty());

    // An Admin owner gains nothing outside the token's environments.
    let admin = user(&pool, AuthRole::Admin, &[]).await;
    let token = caller(&pool, admin, CVES, &[fleet.env_a]).await;
    let result = call(&pool, &token, "list_remediation_work", json!({}))
        .await
        .expect("admin list");
    assert_eq!(system_ids(&result), [host_a.clone(), host_a]);
    assert!(matches!(
        call(
            &pool,
            &token,
            "list_remediation_work",
            json!({"environment_ids": [fleet.env_b]})
        )
        .await,
        Err(ToolError::InvalidArguments(_))
    ));

    // A deactivated environment grants nothing.
    sqlx::query("UPDATE environments SET is_active=false WHERE id=$1")
        .bind(fleet.env_a)
        .execute(&pool)
        .await
        .expect("deactivate A");
    let result = call(&pool, &token, "list_remediation_work", json!({}))
        .await
        .expect("list after deactivation");
    assert!(system_ids(&result).is_empty());
}

#[sqlx::test(migrations = "./migrations")]
async fn list_work_paginates_and_binds_cursor_to_filters(pool: PgPool) {
    let env = environment(&pool, "mcp-page").await;
    let scanned = host(&pool, env, true).await;
    seal_scan(
        &pool,
        &scanned,
        &[
            Finding {
                cve_id: "CVE-2099-0101",
                package: "pkg",
                cvss: Some(9.1),
                fixed_version: None,
            },
            finding("CVE-2099-0102", "pkg"),
            finding("CVE-2099-0103", "pkg"),
        ],
    )
    .await;
    let owner = user(&pool, AuthRole::Viewer, &[env]).await;
    let token = caller(&pool, owner, CVES, &[env]).await;

    let first = call(&pool, &token, "list_remediation_work", json!({"limit": 2}))
        .await
        .expect("first page");
    assert_eq!(first["items"].as_array().map(Vec::len), Some(2));
    let cursor = first["next_cursor"]
        .as_str()
        .expect("next cursor")
        .to_owned();
    let second = call(
        &pool,
        &token,
        "list_remediation_work",
        json!({"limit": 2, "cursor": cursor}),
    )
    .await
    .expect("second page");
    assert_eq!(second["items"].as_array().map(Vec::len), Some(1));
    assert_eq!(second["items"][0]["cve_id"], "CVE-2099-0103");
    assert!(second["next_cursor"].is_null());

    assert!(matches!(
        call(
            &pool,
            &token,
            "list_remediation_work",
            json!({"limit": 2, "cursor": cursor, "severity": ["critical"]}),
        )
        .await,
        Err(ToolError::InvalidArguments(_))
    ));
    assert!(matches!(
        call(
            &pool,
            &token,
            "list_remediation_work",
            json!({"cursor": "not-a-cursor"})
        )
        .await,
        Err(ToolError::InvalidArguments(_))
    ));
    let critical = call(
        &pool,
        &token,
        "list_remediation_work",
        json!({"severity": ["critical"]}),
    )
    .await
    .expect("severity filter");
    assert_eq!(critical["items"].as_array().map(Vec::len), Some(1));
    assert_eq!(critical["items"][0]["cve_id"], "CVE-2099-0101");
}

// ---------------------------------------------------------------------------
// get_system_security_context
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "./migrations")]
async fn system_context_hides_other_environments_and_reports_no_scan(pool: PgPool) {
    let fleet = fleet(&pool).await;
    let unscanned = host(&pool, fleet.env_a, false).await;
    let owner = user(&pool, AuthRole::Operator, &[fleet.env_a, fleet.env_b]).await;
    let token = caller(&pool, owner, CVES, &[fleet.env_a]).await;
    let context = |system_id: Uuid| {
        let pool = pool.clone();
        let token = token.clone();
        async move {
            call(
                &pool,
                &token,
                "get_system_security_context",
                json!({"system_id": system_id}),
            )
            .await
        }
    };

    assert!(matches!(
        context(fleet.host_b.system_id).await,
        Err(ToolError::NotFound)
    ));
    assert!(matches!(
        context(Uuid::new_v4()).await,
        Err(ToolError::NotFound)
    ));
    // A cursor cannot distinguish a hidden system from an absent one.
    assert!(matches!(
        call(
            &pool,
            &token,
            "get_system_security_context",
            json!({"system_id": fleet.host_b.system_id, "cursor": "garbage"}),
        )
        .await,
        Err(ToolError::NotFound)
    ));

    let scanned = context(fleet.host_a.system_id)
        .await
        .expect("scanned context");
    assert_eq!(scanned["system"]["management_mode"], "managed");
    assert_eq!(scanned["system"]["environment"]["id"], json!(fleet.env_a));
    assert_eq!(scanned["evidence"]["authority"], "exact");
    assert_eq!(scanned["evidence"]["current_state"], "exact_current_scan");
    assert_eq!(scanned["vulnerabilities"].as_array().map(Vec::len), Some(2));
    let description = scanned["vulnerabilities"][0]["description"]
        .as_str()
        .expect("description");
    assert!(!description.contains('\u{1b}'));

    let empty = context(unscanned.system_id)
        .await
        .expect("unscanned context");
    assert_eq!(empty["evidence"]["authority"], "no_scan");
    assert_eq!(empty["evidence"]["current_state"], "no_running_report");
    assert!(empty["evidence"]["source"].is_null());
    assert_eq!(empty["vulnerabilities"], json!([]));
    assert_eq!(empty["has_more"], false);
}

// ---------------------------------------------------------------------------
// get_remediation_context
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "./migrations")]
async fn remediation_context_requires_provenance_and_reports_unknown_attribution(pool: PgPool) {
    let fleet = fleet(&pool).await;
    let owner = user(&pool, AuthRole::Operator, &[fleet.env_a, fleet.env_b]).await;
    let arguments = |system_id: Uuid, cve_id: &str, package: &str| json!({"system_id": system_id, "cve_id": cve_id, "package_name": package});

    let cves_only = caller(&pool, owner, CVES, &[fleet.env_a]).await;
    assert!(matches!(
        call(
            &pool,
            &cves_only,
            "get_remediation_context",
            arguments(fleet.host_a.system_id, "CVE-2099-0001", "pkg"),
        )
        .await,
        Err(ToolError::Forbidden(_))
    ));

    let token = caller(&pool, owner, CVES_PROVENANCE, &[fleet.env_a]).await;
    let result = call(
        &pool,
        &token,
        "get_remediation_context",
        arguments(fleet.host_a.system_id, "cve-2099-0001", "pkg"),
    )
    .await
    .expect("remediation context");
    assert_eq!(result["evidence_key"]["cve_id"], "CVE-2099-0001");
    assert_eq!(result["evidence_key"]["package_name"], "pkg");
    assert_eq!(
        result["evidence_key"]["occurrence_derivation_path"],
        json!(fleet.observations_a[0].occurrence_derivation_path)
    );
    assert_eq!(result["package"]["fixed_version"], "1.1");
    assert_eq!(
        result["target"]["store_path"],
        json!(fleet.host_a.store_path)
    );
    assert_eq!(
        result["target"]["commit_hash"],
        json!(fleet.host_a.commit_hash)
    );
    assert_eq!(result["attribution"]["status"], "unknown");
    assert_eq!(result["hints"][0]["kind"], "upgrade_package");
    assert_eq!(result["hints_are_authoritative"], false);
    let text = result.to_string();
    assert!(!text.contains(REPO_SECRET));
    assert!(!text.contains("repo_url"));

    for (system_id, cve_id, package) in [
        (fleet.host_a.system_id, "CVE-2099-0001", "other-package"),
        (fleet.host_b.system_id, "CVE-2099-0001", "pkg"),
        (Uuid::new_v4(), "CVE-2099-0001", "pkg"),
    ] {
        assert!(matches!(
            call(
                &pool,
                &token,
                "get_remediation_context",
                arguments(system_id, cve_id, package),
            )
            .await,
            Err(ToolError::NotFound)
        ));
    }
    assert!(matches!(
        call(
            &pool,
            &token,
            "get_remediation_context",
            arguments(fleet.host_a.system_id, "CVE-99-1", "pkg"),
        )
        .await,
        Err(ToolError::InvalidArguments(_))
    ));
}

#[sqlx::test(migrations = "./migrations")]
async fn remediation_context_finds_a_package_beyond_the_first_inventory_page(pool: PgPool) {
    const CVE_ID: &str = "CVE-2099-0500";
    const PACKAGES: usize = 520;
    let env = environment(&pool, "mcp-bulk").await;
    let scanned = host(&pool, env, true).await;

    // One sealed scan holds more occurrences of one CVE than the largest
    // inventory page. This mirrors `seal_scan` with one set-based insert.
    let scan_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO cve_scans(id,derivation_id,scanner_name,status) VALUES($1,$2,'mcp-test','in_progress')",
    )
    .bind(scan_id)
    .bind(scanned.derivation_id)
    .execute(&pool)
    .await
    .expect("insert scan");
    sqlx::query(
        "INSERT INTO cves(id,cvss_v3_score,description) VALUES($1,5.0,'Bulk inventory finding') \
         ON CONFLICT(id) DO NOTHING",
    )
    .bind(CVE_ID)
    .execute(&pool)
    .await
    .expect("insert CVE");
    sqlx::query(
        "INSERT INTO cve_scan_vulnerability_observations( \
         scan_id,canonical_cve_id,canonical_package_name,observed_package_name, \
         observed_package_version,observed_derivation_path,is_whitelisted,detection_method) \
         SELECT $1,$2,package.name,package.name,'1.0', \
                '/nix/store/' || md5(package.name) || '-' || package.name || '.drv', \
                false,'mcp-test' \
         FROM generate_series(1,$3::int) AS n \
         CROSS JOIN LATERAL (SELECT 'bulk-pkg-' || lpad(n::text,4,'0') AS name) AS package",
    )
    .bind(scan_id)
    .bind(CVE_ID)
    .bind(PACKAGES as i32)
    .execute(&pool)
    .await
    .expect("insert observations");
    sqlx::query(
        "UPDATE cve_scans SET status='completed',completed_at=now(),evidence_schema_version=1 WHERE id=$1",
    )
    .bind(scan_id)
    .execute(&pool)
    .await
    .expect("seal scan");

    let page = SystemCveInventoryPageRequest::from_params(SystemCveInventoryParams {
        limit: Some(500),
        q: Some(CVE_ID.to_owned()),
        ..Default::default()
    })
    .expect("first page request");
    let first = fetch_system_cve_inventory_page(&pool, scanned.system_id, &page)
        .await
        .expect("first inventory page");
    assert!(first.has_more);
    let on_first_page: std::collections::HashSet<&str> = first
        .rows
        .iter()
        .map(|row| row.canonical_package_name.as_str())
        .collect();
    assert_eq!(on_first_page.len(), 500);
    let beyond = (1..=PACKAGES)
        .map(|n| format!("bulk-pkg-{n:04}"))
        .find(|name| !on_first_page.contains(name.as_str()))
        .expect("a package beyond the first page");

    let owner = user(&pool, AuthRole::Viewer, &[env]).await;
    let token = caller(&pool, owner, CVES_PROVENANCE, &[env]).await;
    let result = call(
        &pool,
        &token,
        "get_remediation_context",
        json!({"system_id": scanned.system_id, "cve_id": CVE_ID, "package_name": &beyond}),
    )
    .await
    .expect("remediation context beyond the first page");
    assert_eq!(result["evidence_key"]["cve_id"], CVE_ID);
    assert_eq!(result["evidence_key"]["package_name"], json!(beyond));
    assert_eq!(result["evidence_key"]["scan_id"], json!(scan_id));

    assert!(matches!(
        call(
            &pool,
            &token,
            "get_remediation_context",
            json!({"system_id": scanned.system_id, "cve_id": CVE_ID, "package_name": "bulk-pkg-9999"}),
        )
        .await,
        Err(ToolError::NotFound)
    ));
}

// ---------------------------------------------------------------------------
// POA&M references and get_poam_detail
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "./migrations")]
async fn poam_reads_are_full_or_not_found(pool: PgPool) {
    let fleet = fleet(&pool).await;
    let creator = browser_admin(user(&pool, AuthRole::Admin, &[]).await);
    let owner = user(&pool, AuthRole::Operator, &[fleet.env_a, fleet.env_b]).await;
    let (in_scope, _) = create_poam(&pool, &creator, &fleet.observations_a[0]).await;
    let (hidden, _) = create_poam(&pool, &creator, &fleet.observations_b[0]).await;

    // POA&M references on work items require poams:read and stay in scope.
    let token = caller(&pool, owner, CVES_POAMS, &[fleet.env_a]).await;
    let result = call(
        &pool,
        &token,
        "list_remediation_work",
        json!({"include_poams": true}),
    )
    .await
    .expect("list with POA&Ms");
    assert_eq!(result["items"][0]["poam"]["id"], json!(in_scope));
    assert_eq!(result["items"][0]["poam"]["status"], "open");
    assert!(result["items"][1]["poam"].is_null());

    // A plan spanning a hidden environment is denied as a whole.
    let (shared, revision) = create_poam(&pool, &creator, &fleet.observations_a[1]).await;
    poam_service::link_cve_finding(
        &pool,
        &creator,
        shared,
        AddCveFindingRequest {
            revision,
            observation: fleet.observations_b[1].clone(),
        },
        &SystemClock,
    )
    .await
    .expect("link hidden subject");

    let token = caller(&pool, owner, POAMS, &[fleet.env_a]).await;
    let detail = call(
        &pool,
        &token,
        "get_poam_detail",
        json!({"poam_id": in_scope}),
    )
    .await
    .expect("in-scope detail");
    assert_eq!(detail["visibility"], "full");
    assert_eq!(detail["poam"]["id"], json!(in_scope));
    assert_eq!(
        detail["poam"]["cve_findings"][0]["system_id"],
        json!(fleet.host_a.system_id)
    );
    for poam_id in [hidden, shared, Uuid::new_v4()] {
        assert!(matches!(
            call(
                &pool,
                &token,
                "get_poam_detail",
                json!({"poam_id": poam_id})
            )
            .await,
            Err(ToolError::NotFound)
        ));
    }

    // An Admin owner limited to environment A still cannot read the others.
    let admin = user(&pool, AuthRole::Admin, &[]).await;
    let token = caller(&pool, admin, POAMS, &[fleet.env_a]).await;
    assert!(
        call(
            &pool,
            &token,
            "get_poam_detail",
            json!({"poam_id": in_scope})
        )
        .await
        .is_ok()
    );
    for poam_id in [hidden, shared] {
        assert!(matches!(
            call(
                &pool,
                &token,
                "get_poam_detail",
                json!({"poam_id": poam_id})
            )
            .await,
            Err(ToolError::NotFound)
        ));
    }
}
