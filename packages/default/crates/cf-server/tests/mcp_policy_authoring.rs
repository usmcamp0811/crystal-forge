//! MCP policy authoring regressions (TASK-534.7).
//!
//! Each SQLx test owns an isolated database with the full production
//! migrations. Tokens are issued through the access-token service, so the
//! in-transaction token checks see real token rows. The direct tests call
//! `call_tool` with a caller built from the issued token, the way the
//! transport builds it at admission; this lets a test change the owner or
//! the grant between admission and the write. The HTTP tests speak JSON-RPC
//! to the real `/mcp` handler.
//!
//! Run with:
//! `cargo test -p cf-server --test mcp_policy_authoring -- --test-threads=1`

use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Duration as StdDuration;

use axum::{Router, extract::DefaultBodyLimit, routing::post};
use chrono::{Duration, Utc};
use crystal_forge::api::models::CreateAccessTokenRequest;
use crystal_forge::auth::authorization::RestrictedGrant;
use crystal_forge::compliance::framework_model::FrameworkVersionCanonical;
use crystal_forge::compliance::requirement_model::RequirementVersionCanonical;
use crystal_forge::handlers::agent_request::CFState;
use crystal_forge::mcp;
use crystal_forge::mcp::tools::{ToolCaller, ToolError, call_tool};
use crystal_forge::models::auth_identity::AuthRole;
use crystal_forge::models::deployment_policies::UpdateDeploymentPolicyRequest;
use crystal_forge::queries::auth_identity::{insert_user_environment_membership, sync_user_role};
use crystal_forge::queries::deployment_policies::update_deployment_policy;
use crystal_forge::queries::framework_requirements::{
    insert_framework_version, insert_requirement_version, upsert_framework_lineage,
    upsert_requirement_lineage,
};
use crystal_forge::queries::users::insert_user;
use crystal_forge::services::access_tokens::{self as token_service, AccessTokenSettings};
use crystal_forge::services::policy_authoring::source_sha256;
use crystal_forge::{
    config::ServerConfig, queue::QueueNotifier, server::jobs::BackgroundJobRegistry,
};
use reqwest::Client;
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

const DRAFT: &[&str] = &["policies:read", "policies:draft"];
const IMPORT: &[&str] = &["compliance:preview", "policies:import"];

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

async fn environment(pool: &PgPool) -> Uuid {
    let short = Uuid::new_v4().simple().to_string();
    sqlx::query_scalar("INSERT INTO environments(name) VALUES($1) RETURNING id")
        .bind(format!("authoring-{}", &short[..8]))
        .fetch_one(pool)
        .await
        .expect("insert environment")
}

async fn user(pool: &PgPool, role: AuthRole) -> Uuid {
    let suffix = Uuid::new_v4().simple().to_string();
    let user = insert_user(
        pool,
        &format!("authoring-{suffix}@example.invalid"),
        Some("Policy Authoring Test"),
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

async fn issue(pool: &PgPool, owner: Uuid, scopes: &[&str]) -> Token {
    let environment = environment(pool).await;
    // A Viewer or Operator owner may grant only an environment it belongs to.
    insert_user_environment_membership(pool, owner, environment)
        .await
        .expect("assign membership");
    let name = format!("agent-{}", &Uuid::new_v4().simple().to_string()[..8]);
    let request: CreateAccessTokenRequest = serde_json::from_value(json!({
        "name": name,
        "audience": "mcp",
        "expires_in_days": 30,
        "environment_ids": [environment.to_string()],
        "scopes": scopes,
    }))
    .expect("token request");
    let issued = token_service::create(pool, owner, &request, &settings())
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

/// Issues a token for a new Admin owner and returns it with its caller.
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

/// Row counts of every table that an authoring write changes.
async fn counts(pool: &PgPool) -> (i64, i64, i64, i64) {
    sqlx::query_as(
        "SELECT (SELECT COUNT(*) FROM deployment_policies), \
                (SELECT COUNT(*) FROM deployment_policy_versions), \
                (SELECT COUNT(*) FROM policy_requirement_mappings), \
                (SELECT COUNT(*) FROM admin_audit_events)",
    )
    .fetch_one(pool)
    .await
    .expect("count rows")
}

async fn revision(pool: &PgPool, version_id: Uuid) -> i64 {
    sqlx::query_scalar("SELECT draft_revision FROM deployment_policy_versions WHERE id = $1")
        .bind(version_id)
        .fetch_one(pool)
        .await
        .expect("draft revision")
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

fn uuid(value: &Value) -> Uuid {
    serde_json::from_value(value.clone()).expect("uuid value")
}

/// Inserts a lineage whose trigger-created draft has no creator, and
/// publishes it the way the production publish path leaves it.
async fn published_policy(pool: &PgPool, name: &str) -> (Uuid, Uuid) {
    let policy_id: Uuid = sqlx::query_scalar(
        "INSERT INTO deployment_policies (name, description, policy_type, config, enabled) \
         VALUES ($1, 'Authoring fixture', 'custom_check', $2, false) RETURNING id",
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

/// Creates one framework release with one usable requirement per external
/// ID and returns the requirement version IDs.
async fn requirements(pool: &PgPool, external_ids: &[&str]) -> Vec<Uuid> {
    let key = format!("authoring-fw-{}", Uuid::new_v4());
    let mut tx = pool.begin().await.expect("begin requirement fixture");
    let framework_id = upsert_framework_lineage(&mut tx, "Authoring FW", None, &key, None)
        .await
        .expect("framework lineage");
    let canonical = FrameworkVersionCanonical {
        canonical_source_key: key.clone(),
        canonical_release_key: "V1R1".to_string(),
        version: "V1R1".to_string(),
        publisher: None,
        title: None,
    };
    let framework_version_id =
        insert_framework_version(&mut tx, framework_id, &canonical, None, None)
            .await
            .expect("framework version");
    let mut ids = Vec::new();
    for external_id in external_ids {
        let requirement_id = upsert_requirement_lineage(&mut tx, framework_id, external_id)
            .await
            .expect("requirement lineage");
        let requirement = RequirementVersionCanonical {
            canonical_requirement_key: (*external_id).to_string(),
            external_id: (*external_id).to_string(),
            title: Some(format!("Requirement {external_id}")),
            description: None,
            kind: "rule".to_string(),
            severity: Some("medium".to_string()),
            check_text: None,
            fix_text: None,
            metadata: json!({}),
        };
        ids.push(
            insert_requirement_version(
                &mut tx,
                requirement_id,
                framework_version_id,
                &requirement,
                None,
            )
            .await
            .expect("requirement version"),
        );
    }
    tx.commit().await.expect("commit requirement fixture");
    ids
}

fn multi_rule_config() -> Value {
    json!({
        "mode": "all",
        "rules": [
            {"field_name": "sshEnabled", "expression": "config.services.openssh.enable", "strict": true},
            {"field_name": "firewallEnabled", "expression": "config.networking.firewall.enable", "strict": false}
        ]
    })
}

fn composite_config() -> Value {
    json!({
        "schema_version": 1,
        "mode": "all",
        "rules": [
            {
                "id": "30000000-0000-0000-0000-000000000001",
                "kind": "nixos_option",
                "config": {
                    "path": "services.openssh.banner",
                    "operator": "==",
                    "value_type": "string",
                    "value": "Authorized use only\nSecond line"
                }
            },
            {
                "id": "30000000-0000-0000-0000-000000000002",
                "kind": "cve_block",
                "config": {"severity": "high", "max_allowed": 2}
            }
        ]
    })
}

fn new_lineage(name: &str, config: Value) -> Value {
    json!({
        "mode": "new_lineage",
        "name": name,
        "description": "Agent proposal",
        "policy_type": "custom_check",
        "config": config,
    })
}

fn unique(prefix: &str) -> String {
    format!("{prefix}-{}", &Uuid::new_v4().simple().to_string()[..10])
}

/// One-policy interchange document with explicit portable identities.
fn interchange(lineage_id: Uuid, version_id: Uuid, name: &str, expression: &str) -> String {
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
                "rules": [{"field_name": "check", "expression": expression, "strict": true}]
            },
            "enabled_by_default": true
        }]
    })
    .to_string()
}

fn import_args(document: &str) -> Value {
    json!({
        "document": document,
        "format": "json",
        "expected_source_sha256": source_sha256(document.as_bytes()),
    })
}

// ---------------------------------------------------------------------------
// Drafts
// ---------------------------------------------------------------------------

#[sqlx::test]
async fn admin_token_creates_new_lineage_and_derived_drafts_with_attribution(pool: PgPool) {
    let (token, author) = admin_author(&pool, DRAFT).await;
    let requirement = requirements(&pool, &["V-534701"]).await[0];
    let name = unique("agent-lineage");
    let mut arguments = new_lineage(&name, multi_rule_config());
    arguments["enabled_by_default"] = json!(false);
    arguments["compliance_metadata"] =
        json!({"category": "security", "srg_ids": ["SRG-OS-000001"]});
    arguments["requirement_mappings"] = json!([{
        "requirement_version_id": requirement,
        "relationship": "implements",
        "coverage": "full",
        "rationale": "Agent mapping"
    }]);
    let created = call(&author, &pool, "create_policy_draft", arguments).await;
    let policy_id = uuid(&created["policy_id"]);
    let version_id = uuid(&created["version_id"]);
    assert_eq!(created["mode"], "new_lineage");
    assert_eq!(created["publication_state"], "draft");
    assert_eq!(created["trust_state"], "untrusted");
    assert_eq!(created["draft_revision"], revision(&pool, version_id).await);

    let (lineage_enabled, lineage_name): (bool, String) =
        sqlx::query_as("SELECT enabled, name FROM deployment_policies WHERE id = $1")
            .bind(policy_id)
            .fetch_one(&pool)
            .await
            .expect("lineage");
    assert!(!lineage_enabled, "MCP never enables a lineage");
    assert_eq!(lineage_name, name);
    let row: (
        Option<Uuid>,
        bool,
        Option<Uuid>,
        Option<String>,
        String,
        String,
        bool,
        Value,
    ) = sqlx::query_as(
        "SELECT created_by, created_via_agent, created_access_token_id, \
                    created_access_token_name, trust_state, publication_state, \
                    enabled_by_default, compliance_metadata \
             FROM deployment_policy_versions WHERE id = $1",
    )
    .bind(version_id)
    .fetch_one(&pool)
    .await
    .expect("version row");
    assert_eq!(row.0, Some(token.owner));
    assert!(row.1);
    assert_eq!(row.2, Some(token.id));
    assert_eq!(row.3.as_deref(), Some(token.name.as_str()));
    assert_eq!(row.4, "untrusted");
    assert_eq!(row.5, "draft");
    assert!(!row.6);
    assert_eq!(row.7["category"], "security");
    let mapped: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM policy_requirement_mappings \
         WHERE policy_version_id = $1 AND requirement_version_id = $2 AND provenance = 'manual'",
    )
    .bind(version_id)
    .bind(requirement)
    .fetch_one(&pool)
    .await
    .expect("mapping count");
    assert_eq!(mapped, 1);
    let audits = audit_rows(&pool, version_id).await;
    assert_eq!(audits.len(), 1);
    assert_eq!(audits[0].0, "policy_draft_created");
    assert_eq!(audits[0].1, Some(token.id));
    assert_eq!(audits[0].2["via"], "mcp");
    assert_eq!(audits[0].2["operation"], "new_lineage");
    assert_eq!(audits[0].2["access_token_name"], token.name);
    assert_eq!(audits[0].2["lineage_id"], policy_id.to_string());

    // Derived draft from the published version of another lineage.
    let (published_lineage, published_version) =
        published_policy(&pool, &unique("published")).await;
    let derived = call(
        &author,
        &pool,
        "create_policy_draft",
        json!({"mode": "derive", "policy_id": published_lineage,
               "source_version_id": published_version}),
    )
    .await;
    let derived_id = uuid(&derived["version_id"]);
    assert_eq!(
        derived["derived_from_version_id"],
        published_version.to_string()
    );
    let (created_by, via_agent, token_name): (Option<Uuid>, bool, Option<String>) = sqlx::query_as(
        "SELECT created_by, created_via_agent, created_access_token_name \
             FROM deployment_policy_versions WHERE id = $1",
    )
    .bind(derived_id)
    .fetch_one(&pool)
    .await
    .expect("derived row");
    assert_eq!(created_by, Some(token.owner));
    assert!(via_agent);
    assert_eq!(token_name.as_deref(), Some(token.name.as_str()));
    assert_eq!(
        audit_rows(&pool, derived_id).await[0].2["operation"],
        "derive"
    );

    // A second derivation conflicts and names only the open draft version.
    let message = invalid_message(
        call_err(
            &author,
            &pool,
            "create_policy_draft",
            json!({"mode": "derive", "policy_id": published_lineage}),
        )
        .await,
    );
    assert!(message.starts_with("draft_exists:"), "{message}");
    assert!(message.contains(&derived_id.to_string()), "{message}");
    assert!(!message.contains(&token.owner.to_string()), "{message}");

    // A used name is a conflict, not a second lineage.
    let message = invalid_message(
        call_err(
            &author,
            &pool,
            "create_policy_draft",
            new_lineage(&name, json!({"mode": "all", "rules": []})),
        )
        .await,
    );
    assert!(message.starts_with("name_conflict:"), "{message}");
}

#[sqlx::test]
async fn composite_and_multi_rule_definitions_round_trip_without_loss(pool: PgPool) {
    let (_, author) = admin_author(&pool, DRAFT).await;
    for (policy_type, config, phase) in [
        ("custom_check", multi_rule_config(), "nix-evaluation"),
        ("composite", composite_config(), "multi-phase"),
    ] {
        let created = call(
            &author,
            &pool,
            "create_policy_draft",
            json!({
                "mode": "new_lineage",
                "name": unique(policy_type),
                "policy_type": policy_type,
                "config": config,
            }),
        )
        .await;
        let version_id = uuid(&created["version_id"]);
        let read = call(
            &author,
            &pool,
            "get_compliance_policy",
            json!({"version_id": version_id}),
        )
        .await;
        let stored = read["version"]["config"].clone();
        assert_eq!(read["version"]["policy_type"], policy_type);
        assert_eq!(read["version"]["execution_phase"], phase);
        assert_eq!(read["version"]["draft_revision"], created["draft_revision"]);
        let rules = stored["rules"].as_array().expect("rules");
        assert_eq!(rules.len(), 2, "{policy_type}: {stored}");
        if policy_type == "composite" {
            assert_eq!(stored, config, "composite rules keep every field");
        } else {
            assert_eq!(rules[0]["field_name"], "sshEnabled");
            assert_eq!(rules[0]["expression"], "config.services.openssh.enable");
            assert_eq!(rules[1]["field_name"], "firewallEnabled");
            assert_eq!(rules[1]["strict"], false);
        }

        // The stored definition validates, and writing it back is a no-op.
        let validation = call(
            &author,
            &pool,
            "validate_policy_draft",
            json!({"version_id": version_id}),
        )
        .await;
        assert_eq!(validation["valid"], true, "{validation}");
        let before = counts(&pool).await;
        let rewrite = call(
            &author,
            &pool,
            "update_policy_draft",
            json!({
                "version_id": version_id,
                "draft_revision": created["draft_revision"],
                "config": stored,
                "execution_phase": phase,
            }),
        )
        .await;
        assert_eq!(rewrite["changed"], false, "{policy_type}: {rewrite}");
        assert_eq!(rewrite["draft_revision"], created["draft_revision"]);
        assert_eq!(counts(&pool).await, before);
    }
}

#[sqlx::test]
async fn invalid_definition_fails_without_writing(pool: PgPool) {
    let (_, author) = admin_author(&pool, DRAFT).await;
    let before = counts(&pool).await;
    for arguments in [
        new_lineage(&unique("no-expression"), json!({"strict": false})),
        json!({"mode": "new_lineage", "name": unique("composite-phase"),
               "policy_type": "composite", "config": composite_config(),
               "execution_phase": "nix-evaluation"}),
        json!({"mode": "new_lineage", "name": unique("builtin"),
               "policy_type": "require_cf_agent", "config": {"strict": true}}),
        json!({"mode": "new_lineage", "name": unique("bad-mapping"),
               "policy_type": "custom_check", "config": {"mode": "all", "rules": []},
               "requirement_mappings": [{"requirement_version_id": Uuid::new_v4(),
                                         "relationship": "implements", "coverage": "full"}]}),
        json!({"mode": "new_lineage", "name": unique("bad-metadata"),
               "policy_type": "custom_check", "config": {"mode": "all", "rules": []},
               "compliance_metadata": {"evidence_specs": "not-an-array"}}),
        json!({"mode": "new_lineage", "name": "   ", "policy_type": "custom_check",
               "config": {"mode": "all", "rules": []}}),
    ] {
        let error = call_err(&author, &pool, "create_policy_draft", arguments.clone()).await;
        assert!(
            matches!(error, ToolError::InvalidArguments(_)),
            "{arguments}: {error:?}"
        );
    }
    assert_eq!(counts(&pool).await, before, "no row may be written");
}

#[sqlx::test]
async fn draft_update_requires_current_revision_and_no_op_writes_nothing(pool: PgPool) {
    let (token, author) = admin_author(&pool, DRAFT).await;
    let requirement = requirements(&pool, &["V-534702"]).await[0];
    let created = call(
        &author,
        &pool,
        "create_policy_draft",
        new_lineage(&unique("update"), multi_rule_config()),
    )
    .await;
    let policy_id = uuid(&created["policy_id"]);
    let version_id = uuid(&created["version_id"]);
    let first: i64 = sqlx::query_scalar(
        "UPDATE deployment_policy_versions SET trust_state = 'trusted', \
         trusted_by = created_by, trusted_at = NOW(), trust_review_note = 'Reviewed' \
         WHERE id = $1 RETURNING draft_revision",
    )
    .bind(version_id)
    .fetch_one(&pool)
    .await
    .expect("review draft");
    let no_op = call(
        &author,
        &pool,
        "update_policy_draft",
        json!({"version_id": version_id, "draft_revision": first}),
    )
    .await;
    assert_eq!(no_op["changed"], false);
    let trust: String = sqlx::query_scalar(
        "SELECT trust_state FROM deployment_policy_versions WHERE id = $1",
    )
    .bind(version_id)
    .fetch_one(&pool)
    .await
    .expect("draft trust");
    assert_eq!(trust, "trusted");

    let updated = call(
        &author,
        &pool,
        "update_policy_draft",
        json!({
            "version_id": version_id,
            "draft_revision": first,
            "description": "Refined by the agent",
            "enabled_by_default": false,
            "requirement_mappings": [{"requirement_version_id": requirement,
                                      "relationship": "supports", "coverage": "partial"}],
        }),
    )
    .await;
    assert_eq!(updated["changed"], true);
    let second = updated["draft_revision"].as_i64().expect("revision");
    assert!(second > first, "{first} -> {second}");
    assert_eq!(second, revision(&pool, version_id).await);
    assert_eq!(
        updated["changed_fields"],
        json!(["description", "enabled_by_default", "requirement_mappings"])
    );
    let (description, enabled, lineage_enabled): (Option<String>, bool, bool) = sqlx::query_as(
        "SELECT v.description, v.enabled_by_default, p.enabled \
         FROM deployment_policy_versions v JOIN deployment_policies p ON p.id = v.policy_id \
         WHERE v.id = $1",
    )
    .bind(version_id)
    .fetch_one(&pool)
    .await
    .expect("updated row");
    assert_eq!(description.as_deref(), Some("Refined by the agent"));
    let trust_cleared: bool = sqlx::query_scalar(
        "SELECT trust_state = 'untrusted' AND trusted_by IS NULL AND trusted_at IS NULL \
         AND trust_review_note IS NULL FROM deployment_policy_versions WHERE id = $1",
    )
    .bind(version_id)
    .fetch_one(&pool)
    .await
    .expect("updated draft trust");
    assert!(trust_cleared);
    assert!(!enabled);
    assert!(!lineage_enabled);
    let audits = audit_rows(&pool, version_id).await;
    let update_audit = audits.last().expect("update audit");
    assert_eq!(update_audit.0, "policy_draft_updated");
    assert_eq!(update_audit.1, Some(token.id));
    assert_eq!(update_audit.2["previous_draft_revision"], first);
    assert_eq!(update_audit.2["draft_revision"], second);
    assert!(!update_audit.2.to_string().contains("Refined by the agent"));

    // A stale revision fails and writes nothing.
    let before = counts(&pool).await;
    let message = invalid_message(
        call_err(
            &author,
            &pool,
            "update_policy_draft",
            json!({"version_id": version_id, "draft_revision": first, "description": "Late"}),
        )
        .await,
    );
    assert!(message.starts_with("stale_revision:"), "{message}");
    assert_eq!(revision(&pool, version_id).await, second);
    assert_eq!(counts(&pool).await, before);

    // A no-op keeps the revision and writes no audit event.
    let unchanged = call(
        &author,
        &pool,
        "update_policy_draft",
        json!({
            "version_id": version_id,
            "draft_revision": second,
            "description": "Refined by the agent",
            "enabled_by_default": false,
            "requirement_mappings": [{"requirement_version_id": requirement,
                                      "relationship": "supports", "coverage": "partial"}],
        }),
    )
    .await;
    assert_eq!(unchanged["changed"], false);
    assert_eq!(unchanged["draft_revision"], second);
    assert_eq!(counts(&pool).await, before);
    assert_eq!(revision(&pool, version_id).await, second);

    // A browser edit of the same draft also increments the revision, so the
    // agent's next update with the old revision is stale.
    update_deployment_policy(
        &pool,
        &policy_id,
        &UpdateDeploymentPolicyRequest {
            description: Some("Edited in the browser".into()),
            ..Default::default()
        },
        Some(token.owner),
    )
    .await
    .expect("browser edit")
    .expect("policy exists");
    assert!(revision(&pool, version_id).await > second);
    let message = invalid_message(
        call_err(
            &author,
            &pool,
            "update_policy_draft",
            json!({"version_id": version_id, "draft_revision": second, "description": "Again"}),
        )
        .await,
    );
    assert!(message.starts_with("stale_revision:"), "{message}");
}

#[sqlx::test]
async fn published_versions_and_other_users_drafts_cannot_be_updated(pool: PgPool) {
    let (_, author) = admin_author(&pool, DRAFT).await;
    let (_, other_author) = admin_author(&pool, DRAFT).await;
    let (_, published_version) = published_policy(&pool, &unique("immutable")).await;
    let accepted_revision = revision(&pool, published_version).await;
    let before = counts(&pool).await;
    let message = invalid_message(
        call_err(
            &author,
            &pool,
            "update_policy_draft",
            json!({"version_id": published_version, "draft_revision": accepted_revision,
                   "description": "Changed"}),
        )
        .await,
    );
    assert!(message.starts_with("immutable_version:"), "{message}");
    assert_eq!(counts(&pool).await, before);
    assert_eq!(revision(&pool, published_version).await, accepted_revision);

    let theirs = call(
        &other_author,
        &pool,
        "create_policy_draft",
        new_lineage(&unique("theirs"), json!({"mode": "all", "rules": []})),
    )
    .await;
    let before = counts(&pool).await;
    let error = call_err(
        &author,
        &pool,
        "update_policy_draft",
        json!({"version_id": theirs["version_id"], "draft_revision": theirs["draft_revision"],
               "description": "Hijacked"}),
    )
    .await;
    assert!(
        matches!(&error, ToolError::Forbidden(message) if message == "draft is owned by another user"),
        "{error:?}"
    );
    assert_eq!(counts(&pool).await, before);

    let unknown = call_err(
        &author,
        &pool,
        "update_policy_draft",
        json!({"version_id": Uuid::new_v4(), "draft_revision": 1, "description": "x"}),
    )
    .await;
    assert!(matches!(unknown, ToolError::NotFound), "{unknown:?}");
}

// ---------------------------------------------------------------------------
// Authorization
// ---------------------------------------------------------------------------

#[sqlx::test]
async fn demoted_owner_is_rejected_inside_the_transaction(pool: PgPool) {
    let (token, author) = admin_author(&pool, DRAFT).await;
    let created = call(
        &author,
        &pool,
        "create_policy_draft",
        new_lineage(&unique("demotion"), json!({"mode": "all", "rules": []})),
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
            "create_policy_draft",
            new_lineage(
                &unique("after-demotion"),
                json!({"mode": "all", "rules": []}),
            ),
        ),
        (
            "update_policy_draft",
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
    let demoted = caller(&token, DRAFT, false);
    let error = call_err(
        &demoted,
        &pool,
        "validate_policy_draft",
        json!({"definition": {"policy_type": "custom_check", "config": {"mode": "all", "rules": []}}}),
    )
    .await;
    assert!(matches!(error, ToolError::Forbidden(_)), "{error:?}");
}

#[sqlx::test]
async fn tools_require_their_scope_even_without_tools_list(pool: PgPool) {
    let owner = user(&pool, AuthRole::Admin).await;
    let reader = issue(&pool, owner, &["policies:read"]).await;
    let before = counts(&pool).await;
    let read_only = caller(&reader, &["policies:read"], true);
    for (tool, arguments) in [
        (
            "create_policy_draft",
            new_lineage(&unique("no-scope"), json!({"mode": "all", "rules": []})),
        ),
        (
            "update_policy_draft",
            json!({"version_id": Uuid::new_v4(), "draft_revision": 1}),
        ),
        (
            "validate_policy_draft",
            json!({"definition": {"policy_type": "custom_check", "config": {}}}),
        ),
        (
            "preview_policy_import",
            json!({"document": "{}", "format": "json"}),
        ),
        ("import_policy_draft", import_args("{}")),
    ] {
        let error = call_err(&read_only, &pool, tool, arguments).await;
        assert!(
            matches!(error, ToolError::Forbidden(_)),
            "{tool}: {error:?}"
        );
    }

    // A grant that claims the scope does not help when the token row lacks
    // it: the write transaction checks the row.
    let forged = caller(&reader, DRAFT, true);
    let error = call_err(
        &forged,
        &pool,
        "create_policy_draft",
        new_lineage(&unique("forged"), json!({"mode": "all", "rules": []})),
    )
    .await;
    assert!(matches!(error, ToolError::Forbidden(_)), "{error:?}");
    assert_eq!(counts(&pool).await, before);
}

#[sqlx::test]
async fn validate_reports_structured_errors_without_persistence(pool: PgPool) {
    let (_, author) = admin_author(&pool, DRAFT).await;
    let before = counts(&pool).await;
    let valid = call(
        &author,
        &pool,
        "validate_policy_draft",
        json!({"definition": {"policy_type": "custom_check", "config": multi_rule_config()}}),
    )
    .await;
    assert_eq!(valid["valid"], true, "{valid}");
    assert_eq!(valid["errors"], json!([]));
    assert_eq!(
        valid["note"],
        "Syntax validation does not prove that the check is effective."
    );
    assert!(
        valid["warnings"]
            .as_array()
            .expect("warnings")
            .iter()
            .any(|warning| warning["code"] == "unmapped")
    );

    let invalid = call(
        &author,
        &pool,
        "validate_policy_draft",
        json!({"definition": {
            "policy_type": "composite",
            "config": composite_config(),
            "execution_phase": "post-build",
            "compliance_metadata": [],
            "requirement_mappings": [{"requirement_version_id": Uuid::new_v4(),
                                      "relationship": "owns", "coverage": "full"}]
        }}),
    )
    .await;
    assert_eq!(invalid["valid"], false);
    let errors = invalid["errors"].as_array().expect("errors");
    let codes: BTreeSet<&str> = errors
        .iter()
        .map(|error| error["code"].as_str().expect("code"))
        .collect();
    assert!(codes.contains("invalid_execution_phase"), "{invalid}");
    assert!(codes.contains("invalid_compliance_metadata"), "{invalid}");
    assert!(codes.contains("invalid_mapping"), "{invalid}");
    assert!(codes.contains("unknown_requirement"), "{invalid}");
    for error in errors {
        assert!(error["field"].is_string() && error["message"].is_string());
    }

    let syntax = call(
        &author,
        &pool,
        "validate_policy_draft",
        json!({"definition": {"policy_type": "custom_check", "config": {
            "mode": "all",
            "rules": [{"field_name": "broken", "expression": "config.services.(", "strict": true}]
        }}}),
    )
    .await;
    assert_eq!(syntax["valid"], false, "{syntax}");
    assert_eq!(syntax["errors"][0]["code"], "invalid_config");
    assert_eq!(syntax["errors"][0]["field"], "config");

    let both = call_err(
        &author,
        &pool,
        "validate_policy_draft",
        json!({"version_id": Uuid::new_v4(),
               "definition": {"policy_type": "custom_check", "config": {}}}),
    )
    .await;
    assert!(matches!(both, ToolError::InvalidArguments(_)));
    assert_eq!(counts(&pool).await, before, "validation writes nothing");
}

// ---------------------------------------------------------------------------
// Interchange
// ---------------------------------------------------------------------------

#[sqlx::test]
async fn preview_writes_nothing(pool: PgPool) {
    let owner = user(&pool, AuthRole::Viewer).await;
    let token = issue(&pool, owner, &["compliance:preview"]).await;
    let previewer = caller(&token, &["compliance:preview"], false);
    let lineage_id = Uuid::new_v4();
    let version_id = Uuid::new_v4();
    let document = interchange(
        lineage_id,
        version_id,
        &unique("preview"),
        "config.services.openssh.enable",
    );
    let before = counts(&pool).await;
    for _ in 0..2 {
        let preview = call(
            &previewer,
            &pool,
            "preview_policy_import",
            json!({"document": document, "format": "json"}),
        )
        .await;
        assert_eq!(preview["source_sha256"], source_sha256(document.as_bytes()));
        assert_eq!(preview["policy_count"], 1);
        assert_eq!(
            preview["policies"][0]["reconciliation_state"],
            "new_lineage"
        );
        assert_eq!(preview["policies"][0]["lineage_id"], lineage_id.to_string());
        assert_eq!(preview["has_blocking_conflicts"], false);
    }
    let toml = format!(
        "[[policies]]\nname = \"{}\"\npolicy_type = \"custom_check\"\nenabled_by_default = true\n\n[policies.config]\nmode = \"all\"\nrules = []\n",
        unique("toml")
    );
    let preview = call(
        &previewer,
        &pool,
        "preview_policy_import",
        json!({"document": toml, "format": "toml"}),
    )
    .await;
    assert_eq!(preview["policy_count"], 1);
    assert_eq!(counts(&pool).await, before, "preview writes nothing");
}

#[sqlx::test]
async fn import_creates_untrusted_attributed_drafts_and_reimport_reuses_identities(pool: PgPool) {
    let (token, importer) = admin_author(&pool, IMPORT).await;
    let lineage_id = Uuid::new_v4();
    let version_id = Uuid::new_v4();
    let marker = "config.services.importmarker.enable";
    let document = interchange(lineage_id, version_id, &unique("imported"), marker);
    let sha = source_sha256(document.as_bytes());

    let first = call(
        &importer,
        &pool,
        "import_policy_draft",
        import_args(&document),
    )
    .await;
    assert_eq!(first["source_sha256"], sha);
    assert_eq!(first["created_policy_count"], 1);
    assert_eq!(first["reused_policy_count"], 0);
    assert_eq!(first["policies"][0]["reconciliation_action"], "new_lineage");
    assert_eq!(first["policies"][0]["version_id"], version_id.to_string());
    let row: (
        Uuid,
        String,
        String,
        Option<Uuid>,
        bool,
        Option<String>,
        bool,
    ) = sqlx::query_as(
        "SELECT v.policy_id, v.publication_state, v.trust_state, v.created_by, \
                v.created_via_agent, v.created_access_token_name, p.enabled \
         FROM deployment_policy_versions v JOIN deployment_policies p ON p.id = v.policy_id \
         WHERE v.id = $1",
    )
    .bind(version_id)
    .fetch_one(&pool)
    .await
    .expect("imported version");
    assert_eq!(row.0, lineage_id);
    assert_eq!(row.1, "draft");
    assert_eq!(row.2, "untrusted");
    assert_eq!(row.3, Some(token.owner));
    assert!(row.4);
    assert_eq!(row.5.as_deref(), Some(token.name.as_str()));
    assert!(!row.6, "imported lineages are disabled");
    let audits = audit_rows(&pool, version_id).await;
    assert_eq!(audits.len(), 1);
    assert_eq!(audits[0].0, "policy_interchange_imported");
    assert_eq!(audits[0].1, Some(token.id));
    assert_eq!(audits[0].2["source_digest"], sha);
    assert_eq!(audits[0].2["via"], "mcp");
    assert!(
        !audits[0].2.to_string().contains(marker),
        "the audit event never holds the document"
    );

    // The exact document again reuses the identities and writes no version.
    let before = counts(&pool).await;
    let second = call(
        &importer,
        &pool,
        "import_policy_draft",
        import_args(&document),
    )
    .await;
    assert_eq!(second["created_policy_count"], 0);
    assert_eq!(second["reused_policy_count"], 1);
    assert_eq!(
        second["policies"][0]["reconciliation_action"],
        "exact_match"
    );
    assert_eq!(second["policies"][0]["version_id"], version_id.to_string());
    let after = counts(&pool).await;
    assert_eq!((after.0, after.1, after.2), (before.0, before.1, before.2));
}

#[sqlx::test]
async fn import_rejects_digest_conflicts_source_mismatch_and_bad_formats(pool: PgPool) {
    let (_, importer) = admin_author(&pool, IMPORT).await;
    let lineage_id = Uuid::new_v4();
    let version_id = Uuid::new_v4();
    let name = unique("conflict");
    let original = interchange(lineage_id, version_id, &name, "config.a.enable");
    call(
        &importer,
        &pool,
        "import_policy_draft",
        import_args(&original),
    )
    .await;
    let before = counts(&pool).await;

    // Same immutable identity, different content.
    let changed = interchange(lineage_id, version_id, &name, "config.b.enable");
    let preview = call(
        &importer,
        &pool,
        "preview_policy_import",
        json!({"document": changed, "format": "json"}),
    )
    .await;
    assert_eq!(preview["has_blocking_conflicts"], true);
    let message = invalid_message(
        call_err(
            &importer,
            &pool,
            "import_policy_draft",
            import_args(&changed),
        )
        .await,
    );
    assert!(message.starts_with("import_conflicts:"), "{message}");
    assert!(
        message.contains("INTERCHANGE_VERSION_DIGEST_CONFLICT"),
        "{message}"
    );

    // The digest must be the digest of the sent document.
    let mut mismatch = import_args(&original);
    mismatch["expected_source_sha256"] = json!("0".repeat(64));
    let message =
        invalid_message(call_err(&importer, &pool, "import_policy_draft", mismatch).await);
    assert!(message.starts_with("source_digest_mismatch:"), "{message}");

    // Malformed documents and unsupported formats are rejected.
    let malformed = "{\"policies\": [";
    let message = invalid_message(
        call_err(
            &importer,
            &pool,
            "import_policy_draft",
            import_args(malformed),
        )
        .await,
    );
    assert!(
        message.starts_with("POLICY_INTERCHANGE_INVALID:"),
        "{message}"
    );

    // A policy without enabled_by_default is a validation error, not a
    // database failure: the column is NOT NULL and part of the digest.
    let mut without_default: Value = serde_json::from_str(&interchange(
        Uuid::new_v4(),
        Uuid::new_v4(),
        &unique("nodef"),
        "config.c.enable",
    ))
    .unwrap();
    without_default["policies"][0]
        .as_object_mut()
        .unwrap()
        .remove("enabled_by_default");
    let without_default = without_default.to_string();
    let message = invalid_message(
        call_err(
            &importer,
            &pool,
            "import_policy_draft",
            import_args(&without_default),
        )
        .await,
    );
    assert!(message.contains("enabled_by_default"), "{message}");

    for format in ["xml", "xccdf", "yaml"] {
        let mut arguments = import_args(&original);
        arguments["format"] = json!(format);
        let error = call_err(&importer, &pool, "import_policy_draft", arguments).await;
        assert!(matches!(error, ToolError::InvalidArguments(_)), "{format}");
        let error = call_err(
            &importer,
            &pool,
            "preview_policy_import",
            json!({"document": original, "format": format}),
        )
        .await;
        assert!(matches!(error, ToolError::InvalidArguments(_)), "{format}");
    }
    assert_eq!(
        counts(&pool).await,
        before,
        "rejected imports write nothing"
    );
}

#[sqlx::test]
async fn concurrent_imports_of_one_document_create_one_lineage(pool: PgPool) {
    let (_, importer) = admin_author(&pool, IMPORT).await;
    let lineage_id = Uuid::new_v4();
    let version_id = Uuid::new_v4();
    let document = interchange(
        lineage_id,
        version_id,
        &unique("concurrent"),
        "config.services.openssh.enable",
    );
    let arguments = import_args(&document);
    let (first, second) = tokio::time::timeout(StdDuration::from_secs(60), async {
        tokio::join!(
            call_tool(&pool, &importer, "import_policy_draft", arguments.clone()),
            call_tool(&pool, &importer, "import_policy_draft", arguments.clone()),
        )
    })
    .await
    .expect("concurrent imports finish without a deadlock");
    let first = first.expect("first import");
    let second = second.expect("second import");
    let created = first["created_policy_count"].as_i64().expect("count")
        + second["created_policy_count"].as_i64().expect("count");
    let reused = first["reused_policy_count"].as_i64().expect("count")
        + second["reused_policy_count"].as_i64().expect("count");
    assert_eq!((created, reused), (1, 1));
    let lineages: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM deployment_policies WHERE id = $1")
            .bind(lineage_id)
            .fetch_one(&pool)
            .await
            .expect("lineage count");
    let versions: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM deployment_policy_versions WHERE policy_id = $1")
            .bind(lineage_id)
            .fetch_one(&pool)
            .await
            .expect("version count");
    assert_eq!((lineages, versions), (1, 1));
}

#[sqlx::test]
async fn failed_import_rolls_back_completely(pool: PgPool) {
    let (_, importer) = admin_author(&pool, IMPORT).await;
    let (existing_lineage, _) = published_policy(&pool, &unique("existing")).await;
    let new_lineage_id = Uuid::new_v4();
    // The first policy creates a new lineage. The second adds a version to
    // the existing lineage with the label of its published version, which
    // fails the UNIQUE (policy_id, version) constraint after the first
    // policy was written.
    let document = json!({
        "policies": [
            {
                "lineage_id": new_lineage_id,
                "version_id": Uuid::new_v4(),
                "version": "1.0.0",
                "name": unique("rolled-back"),
                "policy_type": "custom_check",
                "config": {"mode": "all", "rules": []},
                "enabled_by_default": true
            },
            {
                "lineage_id": existing_lineage,
                "version_id": Uuid::new_v4(),
                "version": "0.1.0",
                "name": unique("duplicate-label"),
                "policy_type": "custom_check",
                "config": {"mode": "all", "rules": []},
                "enabled_by_default": true
            }
        ]
    })
    .to_string();
    let before = counts(&pool).await;
    let error = call_err(
        &importer,
        &pool,
        "import_policy_draft",
        import_args(&document),
    )
    .await;
    assert!(matches!(error, ToolError::Internal(_)), "{error:?}");
    assert_eq!(counts(&pool).await, before, "nothing of the import remains");
    let created: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM deployment_policies WHERE id = $1")
        .bind(new_lineage_id)
        .fetch_one(&pool)
        .await
        .expect("lineage count");
    assert_eq!(created, 0);
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
    async fn rpc(&self, token: &Token, method: &str, params: Value) -> Reply {
        let body = json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params});
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

    async fn call(&self, token: &Token, tool: &str, arguments: Value) -> Reply {
        self.rpc(
            token,
            "tools/call",
            json!({"name": tool, "arguments": arguments}),
        )
        .await
    }
}

fn structured(reply: &Reply) -> &Value {
    assert_eq!(reply.status, 200, "{}", reply.text);
    assert_eq!(reply.body["result"]["isError"], false, "{}", reply.text);
    &reply.body["result"]["structuredContent"]
}

fn tool_error(reply: &Reply) -> (String, String) {
    assert_eq!(reply.status, 200, "{}", reply.text);
    assert_eq!(reply.body["result"]["isError"], true, "{}", reply.text);
    let text = reply.body["result"]["content"][0]["text"]
        .as_str()
        .expect("error text");
    let error: Value = serde_json::from_str(text).expect("error JSON");
    (
        error["error"].as_str().expect("error code").to_owned(),
        error["message"].as_str().expect("error message").to_owned(),
    )
}

fn tool_names(reply: &Reply) -> BTreeSet<String> {
    assert_eq!(reply.status, 200, "{}", reply.text);
    reply.body["result"]["tools"]
        .as_array()
        .expect("tools array")
        .iter()
        .map(|tool| tool["name"].as_str().expect("tool name").to_owned())
        .collect()
}

#[sqlx::test]
async fn http_lists_reads_creates_and_updates_a_draft(pool: PgPool) {
    let owner = user(&pool, AuthRole::Admin).await;
    let token = issue(&pool, owner, DRAFT).await;
    let server = start(&pool).await;
    let name = unique("http-draft");

    let created = server
        .call(
            &token,
            "create_policy_draft",
            new_lineage(&name, multi_rule_config()),
        )
        .await;
    let created = structured(&created).clone();
    let version_id = created["version_id"]
        .as_str()
        .expect("version id")
        .to_owned();

    let listed = server
        .call(&token, "list_compliance_policies", json!({"query": name}))
        .await;
    let listed = structured(&listed);
    assert_eq!(listed["items"].as_array().map(Vec::len), Some(1));
    assert_eq!(
        listed["items"][0]["current_draft_version"]["version_id"],
        version_id
    );

    let read = server
        .call(
            &token,
            "get_compliance_policy",
            json!({"version_id": version_id}),
        )
        .await;
    let read = structured(&read);
    let current = read["version"]["draft_revision"].clone();
    assert_eq!(read["version"]["provenance"]["created_by_you"], true);
    assert_eq!(read["version"]["provenance"]["created_via_agent"], true);
    assert_eq!(read["authoring"]["can_edit_draft"], true);

    let updated = server
        .call(
            &token,
            "update_policy_draft",
            json!({"version_id": version_id, "draft_revision": current,
                   "description": "Updated over HTTP"}),
        )
        .await;
    assert_eq!(structured(&updated)["changed"], true);

    let stale = server
        .call(
            &token,
            "update_policy_draft",
            json!({"version_id": version_id, "draft_revision": current,
                   "description": "Stale"}),
        )
        .await;
    let (code, message) = tool_error(&stale);
    assert_eq!(code, "invalid_arguments");
    assert!(message.starts_with("stale_revision:"), "{message}");
}

#[sqlx::test]
async fn http_preview_then_import(pool: PgPool) {
    let owner = user(&pool, AuthRole::Admin).await;
    let token = issue(&pool, owner, IMPORT).await;
    let server = start(&pool).await;
    let version_id = Uuid::new_v4();
    let document = interchange(
        Uuid::new_v4(),
        version_id,
        &unique("http-import"),
        "config.services.openssh.enable",
    );

    let preview = server
        .call(
            &token,
            "preview_policy_import",
            json!({"document": document, "format": "json"}),
        )
        .await;
    let digest = structured(&preview)["source_sha256"]
        .as_str()
        .expect("digest")
        .to_owned();
    let imported = server
        .call(
            &token,
            "import_policy_draft",
            json!({"document": document, "format": "json", "expected_source_sha256": digest}),
        )
        .await;
    let imported = structured(&imported);
    assert_eq!(imported["created_policy_count"], 1);
    assert_eq!(imported["trusted"], false);
    assert_eq!(
        imported["policies"][0]["version_id"],
        version_id.to_string()
    );
    assert_eq!(imported["policies"][0]["trust_state"], "untrusted");
}

#[sqlx::test]
async fn http_revoked_token_gets_401_and_writes_nothing(pool: PgPool) {
    let owner = user(&pool, AuthRole::Admin).await;
    let token = issue(&pool, owner, DRAFT).await;
    let server = start(&pool).await;
    let created = server
        .call(
            &token,
            "create_policy_draft",
            new_lineage(&unique("http-revoke"), json!({"mode": "all", "rules": []})),
        )
        .await;
    let created = structured(&created).clone();
    let version_id = uuid(&created["version_id"]);

    token_service::revoke(&pool, owner, token.id)
        .await
        .expect("revoke token");
    let before = counts(&pool).await;
    let reply = server
        .call(
            &token,
            "update_policy_draft",
            json!({"version_id": version_id, "draft_revision": created["draft_revision"],
                   "description": "After revocation"}),
        )
        .await;
    assert_eq!(reply.status, 401, "{}", reply.text);
    assert_eq!(reply.body["error"]["code"], -32001, "{}", reply.text);
    assert_eq!(counts(&pool).await, before);
    assert_eq!(
        revision(&pool, version_id).await,
        created["draft_revision"].as_i64().expect("revision")
    );
}

#[sqlx::test]
async fn http_offers_no_trust_publish_or_assign_tool_and_audits_denials(pool: PgPool) {
    let owner = user(&pool, AuthRole::Admin).await;
    let every_scope = [
        "cves:read",
        "poams:read",
        "provenance:read",
        "scans:request",
        "poams:write",
        "poams:verify",
        "poams:create",
        "policies:read",
        "policies:draft",
        "policies:import",
        "bundles:read",
        "bundles:draft",
        "compliance:preview",
    ];
    let token = issue(&pool, owner, &every_scope).await;
    let server = start(&pool).await;
    let listed = tool_names(&server.rpc(&token, "tools/list", json!({})).await);
    let expected: BTreeSet<String> = mcp::MCP_TOOL_NAMES
        .iter()
        .map(|name| (*name).to_owned())
        .collect();
    assert_eq!(listed, expected);
    for name in &listed {
        for forbidden in ["trust", "publish", "assign", "enable", "delete"] {
            assert!(!name.contains(forbidden), "{name}");
        }
    }
    let unknown = server
        .call(
            &token,
            "publish_policy_version",
            json!({"version_id": Uuid::new_v4()}),
        )
        .await;
    assert_eq!(unknown.status, 200, "{}", unknown.text);
    assert_eq!(unknown.body["error"]["code"], -32602, "{}", unknown.text);
    assert_eq!(unknown.body["error"]["message"], "Unknown tool");

    // A write without the scope is forbidden and audited without arguments.
    let reader = issue(&pool, owner, &["policies:read"]).await;
    let secret = "argument-text-that-must-not-be-audited";
    let reply = server
        .call(
            &reader,
            "create_policy_draft",
            new_lineage(secret, json!({"mode": "all", "rules": []})),
        )
        .await;
    let (code, _) = tool_error(&reply);
    assert_eq!(code, "forbidden");
    let denied: Vec<Value> = sqlx::query_scalar(
        "SELECT metadata FROM admin_audit_events \
         WHERE action = 'mcp_write_denied' AND access_token_id = $1",
    )
    .bind(reader.id)
    .fetch_all(&pool)
    .await
    .expect("denied rows");
    assert_eq!(denied.len(), 1);
    assert_eq!(denied[0]["tool"], "create_policy_draft");
    assert!(!denied[0].to_string().contains(secret));
}
