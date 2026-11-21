//! MCP compliance read tool regressions (TASK-534.6).
//!
//! Each SQLx test owns an isolated database with the full production
//! migrations. Most tests call `call_tool` directly with a caller built from
//! the owner's current database state, the way `validate_presented_token`
//! builds it for each request. A direct caller can hold `policies:draft` for
//! any owner role, which token creation refuses for a non-Admin owner; the
//! authoring hint tests use that. The last test speaks HTTP to the real
//! `/mcp` handler with a token issued by the access-token service.
//!
//! Run with:
//! `cargo test -p cf-server --test mcp_compliance_read -- --test-threads=1`

use std::sync::Arc;

use axum::{Router, extract::DefaultBodyLimit, routing::post};
use chrono::{Duration, Utc};
use crystal_forge::api::models::{CreateAccessTokenRequest, CreateComplianceBundleRequest};
use crystal_forge::auth::authorization::RestrictedGrant;
use crystal_forge::compliance::framework_model::FrameworkVersionCanonical;
use crystal_forge::compliance::requirement_model::RequirementVersionCanonical;
use crystal_forge::handlers::agent_request::CFState;
use crystal_forge::mcp;
use crystal_forge::mcp::tools::{ToolCaller, ToolError, call_tool};
use crystal_forge::models::auth_identity::AuthRole;
use crystal_forge::queries::auth_identity::{insert_user_environment_membership, sync_user_role};
use crystal_forge::queries::compliance::{PolicyDraftIntent, create_bundle, ensure_policy_draft};
use crystal_forge::queries::framework_requirements::{
    create_policy_mapping, insert_framework_version, insert_requirement_version,
    upsert_framework_lineage, upsert_requirement_lineage,
};
use crystal_forge::queries::users::insert_user;
use crystal_forge::services::access_tokens::{self as token_service, AccessTokenSettings};
use crystal_forge::{
    config::ServerConfig, queue::QueueNotifier, server::jobs::BackgroundJobRegistry,
};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

const POLICIES: &[&str] = &["policies:read"];
const BUNDLES: &[&str] = &["bundles:read"];
const POLICY_TOOLS: [&str; 3] = [
    "list_compliance_policies",
    "get_compliance_policy",
    "list_compliance_requirements",
];
const BUNDLE_TOOLS: [&str; 2] = ["list_compliance_bundles", "get_compliance_bundle"];
/// Keys that would reveal assignment, usage, system or evidence data. No
/// compliance read tool output may contain them at any depth.
const FORBIDDEN_KEYS: [&str; 10] = [
    "assignments",
    "assignment_id",
    "usage",
    "bundle_usage_count",
    "systems",
    "system_id",
    "hostname",
    "environment_id",
    "environments",
    "evidence",
];

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

async fn environment(pool: &PgPool) -> Uuid {
    let short = Uuid::new_v4().simple().to_string();
    sqlx::query_scalar("INSERT INTO environments(name) VALUES($1) RETURNING id")
        .bind(format!("catalog-{}", &short[..8]))
        .fetch_one(pool)
        .await
        .expect("insert environment")
}

async fn user(pool: &PgPool, role: AuthRole) -> Uuid {
    let suffix = Uuid::new_v4().simple().to_string();
    let user = insert_user(
        pool,
        &format!("catalog-{suffix}@example.invalid"),
        Some("Catalog Test"),
    )
    .await
    .expect("insert user");
    sync_user_role(pool, user.id, role)
        .await
        .expect("assign role");
    user.id
}

/// Builds a caller for one request from the owner's current database state.
async fn caller(pool: &PgPool, owner: Uuid, scopes: &[&str]) -> ToolCaller {
    let owner_is_admin: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM user_role_assignments WHERE user_id=$1 AND role='admin')",
    )
    .bind(owner)
    .fetch_one(pool)
    .await
    .expect("owner role");
    let token_id = Uuid::new_v4();
    let environment_id = environment(pool).await;
    ToolCaller {
        token_id,
        owner_user_id: owner,
        owner_is_admin,
        owner_environment_ids: vec![environment_id],
        grant: RestrictedGrant::new(
            token_id,
            Some("mcp".into()),
            scopes.iter().map(|scope| (*scope).to_owned()).collect(),
            [environment_id].into(),
            Utc::now() + Duration::hours(1),
        ),
    }
}

/// Inserts a policy lineage; the lineage trigger creates its draft version.
/// Returns the lineage ID and the draft version ID.
async fn insert_policy(
    pool: &PgPool,
    name: &str,
    policy_type: &str,
    config: Value,
) -> (Uuid, Uuid) {
    let policy_id: Uuid = sqlx::query_scalar(
        "INSERT INTO deployment_policies (name, description, policy_type, config, enabled) \
         VALUES ($1, 'Catalog fixture policy', $2, $3, false) RETURNING id",
    )
    .bind(name)
    .bind(policy_type)
    .bind(config)
    .fetch_one(pool)
    .await
    .expect("insert policy lineage");
    let version_id: Uuid = sqlx::query_scalar(
        "SELECT current_draft_version_id FROM deployment_policies WHERE id = $1",
    )
    .bind(policy_id)
    .fetch_one(pool)
    .await
    .expect("load draft version id");
    (policy_id, version_id)
}

fn custom_config() -> Value {
    json!({"mode": "all", "rules": []})
}

/// Accepts the current draft the way the production publish path leaves it.
async fn publish_current_draft(pool: &PgPool, policy_id: Uuid, version_id: Uuid) {
    let mut tx = pool.begin().await.expect("begin publish");
    sqlx::query("UPDATE deployment_policies SET current_draft_version_id = NULL WHERE id = $1")
        .bind(policy_id)
        .execute(&mut *tx)
        .await
        .expect("clear draft pointer");
    sqlx::query(
        "UPDATE deployment_policy_versions \
         SET publication_state = 'accepted', trust_state = 'trusted', \
             trusted_at = NOW(), published_at = NOW() \
         WHERE id = $1",
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
}

/// Derives a new mutable draft from the published version, created by `actor`.
async fn derive_draft(pool: &PgPool, policy_id: Uuid, actor: Uuid) -> Uuid {
    let mut tx = pool.begin().await.expect("begin derivation");
    let draft = ensure_policy_draft(
        &mut tx,
        policy_id,
        Some(actor),
        None,
        PolicyDraftIntent::CreateExplicit,
    )
    .await
    .expect("derive draft");
    tx.commit().await.expect("commit derivation");
    draft
}

/// Creates one framework release with one requirement per external ID.
/// Returns the framework ID, the release ID and the requirement version IDs.
async fn framework_release(pool: &PgPool, external_ids: &[&str]) -> (Uuid, Uuid, Vec<Uuid>) {
    let key = format!("catalog-fw-{}", Uuid::new_v4());
    let mut tx = pool.begin().await.expect("begin requirement fixture");
    let framework_id = upsert_framework_lineage(&mut tx, "Catalog FW", None, &key, None)
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
    let mut requirement_version_ids = Vec::new();
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
        requirement_version_ids.push(
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
    (framework_id, framework_version_id, requirement_version_ids)
}

async fn call(caller: &ToolCaller, pool: &PgPool, name: &str, arguments: Value) -> Value {
    let result = call_tool(pool, caller, name, arguments).await;
    let value = result.unwrap_or_else(|error| panic!("{name} failed: {error:?}"));
    assert_no_forbidden_keys(&value);
    value
}

async fn call_err(caller: &ToolCaller, pool: &PgPool, name: &str, arguments: Value) -> ToolError {
    match call_tool(pool, caller, name, arguments).await {
        Ok(value) => panic!("{name} unexpectedly succeeded: {value}"),
        Err(error) => error,
    }
}

fn assert_no_forbidden_keys(value: &Value) {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                assert!(
                    !FORBIDDEN_KEYS.contains(&key.as_str()),
                    "output contains {key}"
                );
                assert_no_forbidden_keys(child);
            }
        }
        Value::Array(items) => items.iter().for_each(assert_no_forbidden_keys),
        _ => {}
    }
}

fn ids(items: &Value, key: &str) -> Vec<String> {
    items
        .as_array()
        .expect("items array")
        .iter()
        .map(|item| item[key].as_str().expect("id").to_owned())
        .collect()
}

// ---------------------------------------------------------------------------
// Scope gating
// ---------------------------------------------------------------------------

#[sqlx::test]
async fn compliance_tools_require_their_scope_even_without_tools_list(pool: PgPool) {
    let owner = user(&pool, AuthRole::Viewer).await;
    let (policy_id, _) =
        insert_policy(&pool, "catalog-gate", "custom_check", custom_config()).await;

    let cves_only = caller(&pool, owner, &["cves:read"]).await;
    for tool in POLICY_TOOLS.iter().chain(BUNDLE_TOOLS.iter()) {
        let error = call_err(&cves_only, &pool, tool, json!({})).await;
        assert!(
            matches!(error, ToolError::Forbidden(_)),
            "{tool}: {error:?}"
        );
    }

    let policies = caller(&pool, owner, POLICIES).await;
    for tool in BUNDLE_TOOLS {
        let error = call_err(&policies, &pool, tool, json!({})).await;
        assert!(
            matches!(error, ToolError::Forbidden(_)),
            "{tool}: {error:?}"
        );
    }
    let policy = call(
        &policies,
        &pool,
        "get_compliance_policy",
        json!({"policy_id": policy_id}),
    )
    .await;
    assert_eq!(policy["policy"]["policy_id"], policy_id.to_string());

    let bundles = caller(&pool, owner, BUNDLES).await;
    for tool in POLICY_TOOLS {
        let error = call_err(&bundles, &pool, tool, json!({})).await;
        assert!(
            matches!(error, ToolError::Forbidden(_)),
            "{tool}: {error:?}"
        );
    }
    call(&bundles, &pool, "list_compliance_bundles", json!({})).await;
}

// ---------------------------------------------------------------------------
// Policies
// ---------------------------------------------------------------------------

#[sqlx::test]
async fn policy_list_paginates_filters_and_binds_cursor_to_filters(pool: PgPool) {
    let owner = user(&pool, AuthRole::Viewer).await;
    let tool = caller(&pool, owner, POLICIES).await;
    let prefix = format!("mcpcat-{}", &Uuid::new_v4().simple().to_string()[..8]);
    let (a, _) = insert_policy(
        &pool,
        &format!("{prefix}-a"),
        "custom_check",
        custom_config(),
    )
    .await;
    let (b, b_version) = insert_policy(
        &pool,
        &format!("{prefix}-b"),
        "custom_check",
        custom_config(),
    )
    .await;
    let (c, _) = insert_policy(
        &pool,
        &format!("{prefix}-c"),
        "require_packages",
        json!({"packages": ["vim"]}),
    )
    .await;
    publish_current_draft(&pool, b, b_version).await;

    let first = call(
        &tool,
        &pool,
        "list_compliance_policies",
        json!({"query": prefix.to_uppercase(), "limit": 2}),
    )
    .await;
    assert_eq!(
        ids(&first["items"], "policy_id"),
        [a.to_string(), b.to_string()]
    );
    let published = &first["items"][1];
    assert_eq!(
        published["current_published_version"]["version_id"],
        b_version.to_string()
    );
    assert_eq!(
        published["current_published_version"]["publication_state"],
        "accepted"
    );
    assert!(published["current_draft_version"].is_null());
    let cursor = first["next_cursor"]
        .as_str()
        .expect("next cursor")
        .to_owned();

    let second = call(
        &tool,
        &pool,
        "list_compliance_policies",
        json!({"query": prefix.to_uppercase(), "limit": 2, "cursor": cursor}),
    )
    .await;
    assert_eq!(ids(&second["items"], "policy_id"), [c.to_string()]);
    assert!(second["next_cursor"].is_null());

    // A cursor is bound to the tool and to every filter argument.
    let rebound = call_err(
        &tool,
        &pool,
        "list_compliance_policies",
        json!({"query": prefix, "limit": 2, "cursor": cursor}),
    )
    .await;
    assert!(
        matches!(rebound, ToolError::InvalidArguments(_)),
        "{rebound:?}"
    );
    let other_tool = call_err(
        &caller(&pool, owner, BUNDLES).await,
        &pool,
        "list_compliance_bundles",
        json!({"query": prefix.to_uppercase(), "cursor": cursor}),
    )
    .await;
    assert!(matches!(other_tool, ToolError::InvalidArguments(_)));

    for (filter, expected) in [
        (json!({"publication_state": "accepted"}), vec![b]),
        (json!({"trust_state": "trusted"}), vec![b]),
        (json!({"publication_state": "draft"}), vec![a, c]),
        (json!({"policy_type": "require_packages"}), vec![c]),
    ] {
        let mut arguments = filter.clone();
        arguments["query"] = json!(prefix);
        let page = call(&tool, &pool, "list_compliance_policies", arguments).await;
        let expected: Vec<String> = expected.iter().map(Uuid::to_string).collect();
        assert_eq!(ids(&page["items"], "policy_id"), expected, "{filter}");
    }

    for arguments in [
        json!({"limit": 0}),
        json!({"limit": 101}),
        json!({"query": "   "}),
        json!({"publication_state": "published"}),
        json!({"unknown": true}),
        json!({"cursor": "not-a-cursor"}),
    ] {
        let error = call_err(&tool, &pool, "list_compliance_policies", arguments.clone()).await;
        assert!(
            matches!(error, ToolError::InvalidArguments(_)),
            "{arguments}"
        );
    }
}

#[sqlx::test]
async fn policy_reads_by_lineage_and_exact_version_return_immutable_identities(pool: PgPool) {
    let owner = user(&pool, AuthRole::Viewer).await;
    let tool = caller(&pool, owner, POLICIES).await;
    let author = user(&pool, AuthRole::Admin).await;
    let (_, _, requirements) = framework_release(&pool, &["V-100001"]).await;
    let config = json!({"mode": "all", "rules": [{"expression": "config.services.sshd.enable"}]});
    let (policy_id, published_id) =
        insert_policy(&pool, "catalog-identity", "custom_check", config.clone()).await;
    create_policy_mapping(
        &pool,
        published_id,
        requirements[0],
        "implements",
        "full",
        Some("fixture rationale"),
        "manual",
        author,
    )
    .await
    .expect("create mapping");
    publish_current_draft(&pool, policy_id, published_id).await;
    let draft_id = derive_draft(&pool, policy_id, author).await;

    let lineage = call(
        &tool,
        &pool,
        "get_compliance_policy",
        json!({"policy_id": policy_id}),
    )
    .await;
    assert_eq!(
        lineage["policy"]["current_published_version_id"],
        published_id.to_string()
    );
    assert_eq!(
        lineage["policy"]["current_draft_version_id"],
        draft_id.to_string()
    );
    let published = &lineage["published_version"];
    assert_eq!(published["version_id"], published_id.to_string());
    assert_eq!(published["publication_state"], "accepted");
    assert_eq!(published["trust_state"], "trusted");
    assert_eq!(published["is_current_published"], true);
    assert_eq!(published["is_mutable"], false);
    assert_eq!(published["policy_type"], "custom_check");
    assert_eq!(published["execution_phase"], "nix-evaluation");
    assert_eq!(published["implementation_state"], "native");
    assert_eq!(published["config"], config, "config is returned as stored");
    assert!(
        published["semantic_digest"]
            .as_str()
            .is_some_and(|d| !d.is_empty())
    );
    let mapping = &published["requirement_mappings"][0];
    assert_eq!(
        mapping["requirement_version_id"],
        requirements[0].to_string()
    );
    assert_eq!(mapping["relationship"], "implements");
    assert_eq!(mapping["coverage"], "full");
    assert_eq!(mapping["requirement_external_id"], "V-100001");
    let draft = &lineage["draft_version"];
    assert_eq!(draft["version_id"], draft_id.to_string());
    assert_eq!(draft["is_mutable"], true);
    assert_eq!(draft["provenance"]["created_by"], author.to_string());
    assert_eq!(draft["provenance"]["created_by_you"], false);
    assert_eq!(
        draft["provenance"]["derived_from_version_id"],
        published_id.to_string()
    );
    assert_eq!(
        draft["requirement_mappings"].as_array().map(Vec::len),
        Some(1),
        "a derived draft inherits the mappings"
    );
    let mut listed = ids(&lineage["versions"], "version_id");
    listed.sort();
    let mut expected = vec![published_id.to_string(), draft_id.to_string()];
    expected.sort();
    assert_eq!(listed, expected);
    assert_eq!(lineage["versions_truncated"], false);
    assert!(lineage.get("version").is_none());

    // The exact version read returns the same immutable identity.
    let exact = call(
        &tool,
        &pool,
        "get_compliance_policy",
        json!({"version_id": published_id}),
    )
    .await;
    assert_eq!(exact["version"]["version_id"], published_id.to_string());
    assert_eq!(
        exact["version"]["semantic_digest"],
        published["semantic_digest"]
    );
    assert_eq!(exact["version"]["config"], config);
    assert_eq!(exact["policy"]["policy_id"], policy_id.to_string());
    assert!(exact.get("published_version").is_none());

    for arguments in [
        json!({"policy_id": Uuid::new_v4()}),
        json!({"version_id": Uuid::new_v4()}),
    ] {
        let error = call_err(&tool, &pool, "get_compliance_policy", arguments.clone()).await;
        assert!(
            matches!(error, ToolError::NotFound),
            "{arguments}: {error:?}"
        );
    }
    for arguments in [
        json!({}),
        json!({"policy_id": policy_id, "version_id": published_id}),
        json!({"policy_id": "not-a-uuid"}),
    ] {
        let error = call_err(&tool, &pool, "get_compliance_policy", arguments.clone()).await;
        assert!(
            matches!(error, ToolError::InvalidArguments(_)),
            "{arguments}"
        );
    }
}

#[sqlx::test]
async fn draft_authoring_hints_follow_scope_admin_and_draft_creator(pool: PgPool) {
    let admin = user(&pool, AuthRole::Admin).await;
    let other_admin = user(&pool, AuthRole::Admin).await;
    let operator = user(&pool, AuthRole::Operator).await;
    let (owned, owned_v1) =
        insert_policy(&pool, "catalog-owned", "custom_check", custom_config()).await;
    publish_current_draft(&pool, owned, owned_v1).await;
    let owned_draft = derive_draft(&pool, owned, admin).await;
    let (operator_policy, operator_v1) =
        insert_policy(&pool, "catalog-operator", "custom_check", custom_config()).await;
    publish_current_draft(&pool, operator_policy, operator_v1).await;
    derive_draft(&pool, operator_policy, operator).await;
    let (published_only, published_v1) =
        insert_policy(&pool, "catalog-published", "custom_check", custom_config()).await;
    publish_current_draft(&pool, published_only, published_v1).await;

    let hints = |result: Value| result["authoring"].clone();
    let get = |policy_id: Uuid| json!({"policy_id": policy_id});
    let with_draft = &["policies:read", "policies:draft"];

    // Admin owner, draft scope, own draft: edit, not derive.
    let author = caller(&pool, admin, with_draft).await;
    let own = hints(call(&author, &pool, "get_compliance_policy", get(owned)).await);
    assert_eq!(own["open_draft_version_id"], owned_draft.to_string());
    assert_eq!(own["open_draft_created_by_you"], true);
    assert_eq!(own["can_edit_draft"], true);
    assert_eq!(own["can_derive_draft"], false);
    assert_eq!(own["draft_scope"], "policies:draft");
    // No open draft and a published source: derive.
    let fresh = hints(call(&author, &pool, "get_compliance_policy", get(published_only)).await);
    assert!(fresh["open_draft_version_id"].is_null());
    assert_eq!(fresh["can_edit_draft"], false);
    assert_eq!(fresh["can_derive_draft"], true);

    // Without the draft scope, nothing is permitted.
    let reader = caller(&pool, admin, POLICIES).await;
    let read_only = hints(call(&reader, &pool, "get_compliance_policy", get(owned)).await);
    assert_eq!(read_only["open_draft_created_by_you"], true);
    assert_eq!(read_only["can_edit_draft"], false);
    let read_only = hints(call(&reader, &pool, "get_compliance_policy", get(published_only)).await);
    assert_eq!(read_only["can_derive_draft"], false);

    // Another Admin's draft blocks editing and deriving.
    let other = caller(&pool, other_admin, with_draft).await;
    let held = hints(call(&other, &pool, "get_compliance_policy", get(owned)).await);
    assert_eq!(held["open_draft_version_id"], owned_draft.to_string());
    assert_eq!(held["open_draft_created_by_you"], false);
    assert_eq!(held["can_edit_draft"], false);
    assert_eq!(held["can_derive_draft"], false);

    // A non-Admin owner never gets authoring, even holding the scope and
    // having created the draft (for example an owner demoted from Admin).
    let demoted = caller(&pool, operator, with_draft).await;
    let operator_hints = hints(
        call(
            &demoted,
            &pool,
            "get_compliance_policy",
            get(operator_policy),
        )
        .await,
    );
    assert_eq!(operator_hints["open_draft_created_by_you"], true);
    assert_eq!(operator_hints["can_edit_draft"], false);
    let operator_hints = hints(
        call(
            &demoted,
            &pool,
            "get_compliance_policy",
            get(published_only),
        )
        .await,
    );
    assert_eq!(operator_hints["can_derive_draft"], false);

    // The exact version read reports the same lineage hints.
    let exact = hints(
        call(
            &author,
            &pool,
            "get_compliance_policy",
            json!({"version_id": owned_v1}),
        )
        .await,
    );
    assert_eq!(exact["can_edit_draft"], true);
}

// ---------------------------------------------------------------------------
// Bundles
// ---------------------------------------------------------------------------

#[sqlx::test]
async fn bundle_reads_return_exact_ordered_membership(pool: PgPool) {
    let owner = user(&pool, AuthRole::Operator).await;
    let tool = caller(&pool, owner, BUNDLES).await;
    let (_, framework_version_id, requirements) =
        framework_release(&pool, &["V-200001", "V-200002"]).await;
    let (first_policy, first_version) =
        insert_policy(&pool, "catalog-member-one", "custom_check", custom_config()).await;
    let (second_policy, second_version) =
        insert_policy(&pool, "catalog-member-two", "custom_check", custom_config()).await;
    let prefix = format!("bundlecat-{}", &Uuid::new_v4().simple().to_string()[..8]);
    let request = |name: String| CreateComplianceBundleRequest {
        name,
        framework: "Catalog FW".to_string(),
        version: Some("1.0".to_string()),
        description: Some("Catalog fixture bundle".to_string()),
        layer: None,
        required_envs: Vec::new(),
        // The second policy is listed first: membership order is request order.
        policy_ids: vec![second_policy, first_policy],
        requirement_version_ids: vec![requirements[1], requirements[0]],
    };
    let bundle = create_bundle(&pool, request(format!("{prefix}-a")))
        .await
        .expect("create bundle");
    let other = create_bundle(&pool, request(format!("{prefix}-b")))
        .await
        .expect("create second bundle");

    let lineage = call(
        &tool,
        &pool,
        "get_compliance_bundle",
        json!({"bundle_id": bundle.id}),
    )
    .await;
    assert!(lineage["published_version"].is_null());
    let draft = &lineage["draft_version"];
    let draft_id = draft["version_id"]
        .as_str()
        .expect("draft version id")
        .to_owned();
    assert_eq!(lineage["bundle"]["current_draft_version_id"], draft_id);
    assert_eq!(draft["is_mutable"], true);
    assert_eq!(draft["framework"], "Catalog FW");
    let policies = draft["policies"].as_array().expect("policies");
    assert_eq!(
        policies
            .iter()
            .map(|member| (
                member["policy_order"].as_i64().unwrap(),
                member["policy_version_id"].as_str().unwrap().to_owned(),
                member["policy_id"].as_str().unwrap().to_owned(),
            ))
            .collect::<Vec<_>>(),
        [
            (0, second_version.to_string(), second_policy.to_string()),
            (1, first_version.to_string(), first_policy.to_string()),
        ]
    );
    assert_eq!(
        ids(&draft["requirements"], "requirement_version_id"),
        [requirements[1].to_string(), requirements[0].to_string()]
    );
    assert_eq!(
        draft["requirements"][0]["framework_version_id"],
        framework_version_id.to_string()
    );
    assert_eq!(lineage["authoring"]["draft_scope"], "bundles:draft");
    assert_eq!(lineage["authoring"]["can_edit_draft"], false);

    let exact = call(
        &tool,
        &pool,
        "get_compliance_bundle",
        json!({"version_id": draft_id}),
    )
    .await;
    assert_eq!(exact["version"]["version_id"], draft_id);
    assert_eq!(exact["version"]["policies"], draft["policies"]);
    assert_eq!(
        exact["version"]["semantic_digest"],
        draft["semantic_digest"]
    );

    // The list is ordered by name and pages with a filter-bound cursor.
    let first = call(
        &tool,
        &pool,
        "list_compliance_bundles",
        json!({"query": prefix, "limit": 1}),
    )
    .await;
    assert_eq!(ids(&first["items"], "bundle_id"), [bundle.id.to_string()]);
    assert_eq!(
        first["items"][0]["current_draft_version"]["version_id"],
        draft_id
    );
    assert!(first["items"][0]["current_published_version"].is_null());
    let cursor = first["next_cursor"]
        .as_str()
        .expect("next cursor")
        .to_owned();
    let second = call(
        &tool,
        &pool,
        "list_compliance_bundles",
        json!({"query": prefix, "limit": 1, "cursor": cursor}),
    )
    .await;
    assert_eq!(ids(&second["items"], "bundle_id"), [other.id.to_string()]);
    assert!(second["next_cursor"].is_null());
    let rebound = call_err(
        &tool,
        &pool,
        "list_compliance_bundles",
        json!({"query": "other", "limit": 1, "cursor": cursor}),
    )
    .await;
    assert!(matches!(rebound, ToolError::InvalidArguments(_)));

    for arguments in [
        json!({"bundle_id": Uuid::new_v4()}),
        json!({"version_id": Uuid::new_v4()}),
    ] {
        let error = call_err(&tool, &pool, "get_compliance_bundle", arguments.clone()).await;
        assert!(
            matches!(error, ToolError::NotFound),
            "{arguments}: {error:?}"
        );
    }
    let both = call_err(
        &tool,
        &pool,
        "get_compliance_bundle",
        json!({"bundle_id": bundle.id, "version_id": draft_id}),
    )
    .await;
    assert!(matches!(both, ToolError::InvalidArguments(_)));
}

// ---------------------------------------------------------------------------
// Requirements
// ---------------------------------------------------------------------------

#[sqlx::test]
async fn requirement_discovery_lists_frameworks_searches_and_returns_mappings(pool: PgPool) {
    let owner = user(&pool, AuthRole::Viewer).await;
    let author = user(&pool, AuthRole::Admin).await;
    let tool = caller(&pool, owner, POLICIES).await;
    let (framework_id, framework_version_id, requirements) =
        framework_release(&pool, &["V-300001", "V-300002", "V-399999"]).await;
    let (policy_id, version_id) =
        insert_policy(&pool, "catalog-mapped", "custom_check", custom_config()).await;
    create_policy_mapping(
        &pool,
        version_id,
        requirements[0],
        "supports",
        "partial",
        None,
        "manual",
        author,
    )
    .await
    .expect("create mapping");

    // Framework discovery pages through every framework with its versions.
    let mut found = None;
    let mut cursor: Option<String> = None;
    loop {
        let mut arguments = json!({"limit": 1});
        if let Some(cursor) = &cursor {
            arguments["cursor"] = json!(cursor);
        }
        let page = call(&tool, &pool, "list_compliance_requirements", arguments).await;
        for item in page["items"].as_array().expect("framework items") {
            if item["framework_id"] == framework_id.to_string() {
                found = Some(item.clone());
            }
        }
        match page["next_cursor"].as_str() {
            Some(next) => cursor = Some(next.to_owned()),
            None => break,
        }
    }
    let framework = found.expect("fixture framework listed");
    assert_eq!(
        framework["versions"][0]["framework_version_id"],
        framework_version_id.to_string()
    );
    assert_eq!(framework["versions"][0]["requirement_count"], 3);

    // Requirement search within one framework version.
    let searched = call(
        &tool,
        &pool,
        "list_compliance_requirements",
        json!({"framework_version_id": framework_version_id, "query": "V-3999"}),
    )
    .await;
    assert_eq!(
        ids(&searched["items"], "requirement_version_id"),
        [requirements[2].to_string()]
    );
    let first = call(
        &tool,
        &pool,
        "list_compliance_requirements",
        json!({"framework_version_id": framework_version_id, "limit": 2}),
    )
    .await;
    assert_eq!(
        ids(&first["items"], "requirement_version_id"),
        [requirements[0].to_string(), requirements[1].to_string()]
    );
    let cursor = first["next_cursor"]
        .as_str()
        .expect("next cursor")
        .to_owned();
    let second = call(
        &tool,
        &pool,
        "list_compliance_requirements",
        json!({"framework_version_id": framework_version_id, "limit": 2, "cursor": cursor}),
    )
    .await;
    assert_eq!(
        ids(&second["items"], "requirement_version_id"),
        [requirements[2].to_string()]
    );
    assert!(second["next_cursor"].is_null());

    // Mappings of one requirement version.
    let mapped = call(
        &tool,
        &pool,
        "list_compliance_requirements",
        json!({"requirement_version_id": requirements[0]}),
    )
    .await;
    assert_eq!(mapped["requirement"]["external_id"], "V-300001");
    assert_eq!(
        mapped["requirement"]["framework_version_id"],
        framework_version_id.to_string()
    );
    let mapping = &mapped["mappings"][0];
    assert_eq!(mapping["policy_version_id"], version_id.to_string());
    assert_eq!(mapping["policy_id"], policy_id.to_string());
    assert_eq!(mapping["relationship"], "supports");
    assert_eq!(mapping["coverage"], "partial");
    assert_eq!(mapping["trust_state"], "trusted");
    assert_eq!(mapped["mappings_truncated"], false);

    for arguments in [
        json!({"requirement_version_id": Uuid::new_v4()}),
        json!({"framework_version_id": Uuid::new_v4()}),
    ] {
        let error = call_err(
            &tool,
            &pool,
            "list_compliance_requirements",
            arguments.clone(),
        )
        .await;
        assert!(
            matches!(error, ToolError::NotFound),
            "{arguments}: {error:?}"
        );
    }
    for arguments in [
        json!({"query": "V-3"}),
        json!({"requirement_version_id": requirements[0], "limit": 5}),
        json!({"framework_version_id": framework_version_id, "query": "V-3", "cursor": cursor}),
    ] {
        let error = call_err(
            &tool,
            &pool,
            "list_compliance_requirements",
            arguments.clone(),
        )
        .await;
        assert!(
            matches!(error, ToolError::InvalidArguments(_)),
            "{arguments}"
        );
    }
}

// ---------------------------------------------------------------------------
// HTTP
// ---------------------------------------------------------------------------

async fn start(pool: &PgPool, config: ServerConfig) -> String {
    let state = CFState::new(
        pool.clone(),
        config,
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
    tokio::spawn(async move { axum::serve(listener, app).await.expect("serve MCP") });
    format!("http://{address}/mcp")
}

async fn rpc(url: &str, secret: &str, method: &str, params: Value) -> Value {
    let body = json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params});
    let response = reqwest::Client::new()
        .post(url)
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {secret}"))
        .body(body.to_string())
        .send()
        .await
        .expect("send MCP request");
    assert_eq!(response.status().as_u16(), 200);
    let text = response.text().await.expect("read response body");
    serde_json::from_str(&text).expect("JSON-RPC body")
}

#[sqlx::test]
async fn compliance_tools_are_listed_and_called_over_http(pool: PgPool) {
    let config = ServerConfig {
        mcp_enable: true,
        mcp_public_url: Some("https://forge.example.com/mcp".to_owned()),
        ..ServerConfig::default()
    };
    let owner = user(&pool, AuthRole::Viewer).await;
    let env = environment(&pool).await;
    insert_user_environment_membership(&pool, owner, env)
        .await
        .expect("assign environment membership");
    let settings = AccessTokenSettings::from_server_config(&config, 86_400);
    let issue = |scopes: &[&str]| CreateAccessTokenRequest {
        name: Some(format!("catalog {}", scopes.join(" "))),
        audience: Some("mcp".to_owned()),
        expires_in_days: Some(30),
        environment_ids: Some(vec![env.to_string()]),
        scopes: Some(scopes.iter().map(|scope| (*scope).to_owned()).collect()),
        confirm_production_write: None,
    };
    let reader = token_service::create(
        &pool,
        owner,
        &issue(&["policies:read", "bundles:read"]),
        &settings,
    )
    .await
    .expect("issue compliance token");
    let bundles_only = token_service::create(&pool, owner, &issue(&["bundles:read"]), &settings)
        .await
        .expect("issue bundles token");
    let (policy_id, _) =
        insert_policy(&pool, "catalog-http", "custom_check", custom_config()).await;
    let url = start(&pool, config).await;

    let listed = rpc(&url, reader.secret.expose(), "tools/list", json!({})).await;
    let mut names: Vec<String> = listed["result"]["tools"]
        .as_array()
        .expect("tools")
        .iter()
        .map(|tool| tool["name"].as_str().expect("tool name").to_owned())
        .collect();
    names.sort();
    let mut expected: Vec<String> = POLICY_TOOLS
        .iter()
        .chain(BUNDLE_TOOLS.iter())
        .map(|name| (*name).to_owned())
        .collect();
    expected.sort();
    assert_eq!(names, expected);

    let called = rpc(
        &url,
        reader.secret.expose(),
        "tools/call",
        json!({"name": "get_compliance_policy", "arguments": {"policy_id": policy_id}}),
    )
    .await;
    let result = &called["result"];
    assert_eq!(result["isError"], false, "{called}");
    assert_eq!(
        result["structuredContent"]["policy"]["policy_id"],
        policy_id.to_string()
    );
    assert_no_forbidden_keys(&result["structuredContent"]);

    // A token without policies:read cannot call the tool, even though
    // tools/list never offered it.
    let denied = rpc(
        &url,
        bundles_only.secret.expose(),
        "tools/call",
        json!({"name": "get_compliance_policy", "arguments": {"policy_id": policy_id}}),
    )
    .await;
    assert_eq!(denied["result"]["isError"], true, "{denied}");
    let text = denied["result"]["content"][0]["text"]
        .as_str()
        .expect("error text");
    let error: Value = serde_json::from_str(text).expect("error JSON");
    assert_eq!(error["error"], "forbidden");
}
