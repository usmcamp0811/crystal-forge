//! Reads sealed configuration evidence and checks its independent runtime authority.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::api::models::SystemCveInventorySource;
use crate::auth::authorization::CredentialIdentity;
use crate::queries::{access_tokens, cve_scans, cves};

use super::tools::{
    Severity, ToolCaller, ToolError, cursor_schema, inventory_error, inventory_request,
    limit_schema, page_limit, parse_arguments, sanitize_description, severity_names,
    severity_schema, system_id_schema,
};

const MAX_NAME_CHARS: usize = 200;
const SCOPES: &[&str] = &["cves:read"];

/// Returns the bounded exact-name discovery schema.
pub(super) fn find_systems_schema() -> Value {
    json!({
        "type": "object", "additionalProperties": false, "required": ["query"],
        "properties": {
            "query": {"type": "string", "minLength": 1, "maxLength": MAX_NAME_CHARS,
                "description": "Exact hostname or effective configuration name, ignoring case."},
            "limit": limit_schema(),
        }
    })
}

/// Returns the exact scan and page selection schema.
pub(super) fn list_scan_findings_schema() -> Value {
    json!({
        "type": "object", "additionalProperties": false, "required": ["system_id"],
        "properties": {
            "system_id": system_id_schema(),
            "scan_id": {"type": "string", "format": "uuid",
                "description": "Exact sealed scan ID. Omit to select registered flake HEAD."},
            "severity": severity_schema(),
            "package": {"type": "string", "minLength": 1, "maxLength": MAX_NAME_CHARS,
                "description": "Exact, case-sensitive canonical package name from the scan inventory."},
            "cursor": cursor_schema(),
            "limit": limit_schema(),
        }
    })
}

fn bounded_name(value: &str, field: &str) -> Result<String, ToolError> {
    let value = value.trim();
    if value.is_empty() || value.contains('\0') || value.chars().count() > MAX_NAME_CHARS {
        return Err(ToolError::InvalidArguments(format!(
            "{field} must contain 1 to {MAX_NAME_CHARS} characters and no NUL"
        )));
    }
    Ok(value.to_owned())
}

fn db_error(error: sqlx::Error) -> ToolError {
    ToolError::Internal(error.into())
}

async fn scoped_read<'a>(
    pool: &'a PgPool,
    caller: &ToolCaller,
) -> Result<(Transaction<'a, Postgres>, Vec<Uuid>), ToolError> {
    let forbidden = || ToolError::Forbidden("This token does not grant cves:read".into());
    let credential = CredentialIdentity::Restricted(caller.grant.clone());
    let admitted = credential
        .reading_environments(
            caller.owner_is_admin,
            caller.owner_environment_ids.clone(),
            SCOPES,
            Utc::now(),
        )
        .ok_or_else(forbidden)?;

    // SECURITY: The same snapshot covers live authorization, system identity,
    // HEAD, scan selection, counts, and rows. The refresh may only narrow the
    // admitted scope, including when the owner gains Admin during the call.
    let mut tx = pool.begin().await.map_err(db_error)?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
        .execute(&mut *tx)
        .await
        .map_err(db_error)?;
    let owner = access_tokens::owner_access(&mut *tx, caller.owner_user_id)
        .await
        .map_err(ToolError::Internal)?
        .filter(|owner| owner.is_active && owner.has_any_role())
        .ok_or_else(forbidden)?;
    let live_token: bool = sqlx::query_scalar(
        r#"SELECT EXISTS (
            SELECT 1 FROM api_access_tokens t JOIN api_access_token_scopes s ON s.token_id=t.id
            WHERE t.id=$1 AND t.user_id=$2 AND t.audience='mcp' AND t.revoked_at IS NULL
              AND t.expires_at>clock_timestamp() AND s.scope='cves:read')"#,
    )
    .bind(caller.grant.token_id())
    .bind(caller.owner_user_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(db_error)?;
    if !live_token || caller.grant.token_id() != caller.token_id {
        return Err(forbidden());
    }
    let current = credential
        .reading_environments(owner.is_admin(), owner.environment_ids, SCOPES, Utc::now())
        .ok_or_else(forbidden)?;
    let bounded: Vec<_> = current
        .into_iter()
        .filter(|id| admitted.contains(id))
        .collect();
    let active = sqlx::query_scalar(
        r#"SELECT e.id FROM api_access_token_environments te
           JOIN environments e ON e.id=te.environment_id AND e.is_active IS TRUE
           WHERE te.token_id=$1 AND e.id=ANY($2)"#,
    )
    .bind(caller.token_id)
    .bind(&bounded)
    .fetch_all(&mut *tx)
    .await
    .map_err(db_error)?;
    Ok((tx, active))
}

const SCOPED_SYSTEMS: &str = r#"WITH scoped AS (
    SELECT s.id AS system_id,s.hostname,s.flake_id,s.environment_id,e.name AS environment_name,
           s.management_mode::text AS management_mode,s.is_active IS TRUE AS is_active,
           COALESCE(NULLIF(BTRIM(s.system_configuration_name), ''),s.hostname) AS configuration_name
    FROM systems s JOIN environments e ON e.id=s.environment_id
    WHERE s.environment_id=ANY($1)
)"#;

#[derive(sqlx::FromRow, Serialize)]
struct SystemMatch {
    system_id: Uuid,
    hostname: String,
    configuration_name: String,
    flake_id: Option<i32>,
    #[sqlx(flatten)]
    environment: Environment,
    management_mode: String,
    is_active: bool,
}

#[derive(sqlx::FromRow, Serialize)]
struct Environment {
    #[sqlx(rename = "environment_id")]
    id: Uuid,
    #[sqlx(rename = "environment_name")]
    name: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FindArgs {
    query: String,
    limit: Option<i64>,
}

/// Returns an ordered, bounded set of matches in the current token scope.
///
/// # Errors
/// Returns forbidden, invalid arguments, or a database error.
pub(super) async fn find_systems(
    pool: &PgPool,
    caller: &ToolCaller,
    arguments: Value,
) -> Result<Value, ToolError> {
    let (mut tx, environments) = scoped_read(pool, caller).await?;
    let args: FindArgs = parse_arguments(arguments)?;
    let query = bounded_name(&args.query, "query")?;
    let limit = page_limit(args.limit)?;
    let sql = format!(
        r#"{SCOPED_SYSTEMS} SELECT * FROM scoped
           WHERE lower(hostname)=lower($2) OR lower(configuration_name)=lower($2)
           ORDER BY lower(hostname) COLLATE "C",system_id LIMIT $3"#,
    );
    let mut systems = sqlx::query_as::<_, SystemMatch>(&sql)
        .bind(&environments)
        .bind(query)
        .bind(limit + 1)
        .fetch_all(&mut *tx)
        .await
        .map_err(db_error)?;
    let has_more = systems.len() > limit as usize;
    systems.truncate(limit as usize);
    tx.commit().await.map_err(db_error)?;
    Ok(json!({"systems": systems, "has_more": has_more}))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ScanArgs {
    system_id: Uuid,
    scan_id: Option<Uuid>,
    severity: Option<Vec<Severity>>,
    package: Option<String>,
    cursor: Option<String>,
    limit: Option<i64>,
}

#[derive(sqlx::FromRow, Serialize)]
struct Head {
    snapshot_ready: bool,
    branch: String,
    commit_id: Option<i32>,
    commit_hash: Option<String>,
    configuration_derivation_id: Option<i32>,
    execution_derivation_id: Option<i32>,
    target_ready: bool,
}

#[derive(sqlx::FromRow)]
struct SealedScan {
    scan_id: Uuid,
    completed_at: DateTime<Utc>,
    scanner_name: String,
    scanner_version: Option<String>,
    derivation_id: i32,
    commit_id: i32,
    commit_hash: String,
    derivation_path: Option<String>,
    scanned_system_store_path: Option<String>,
    raw_counts: Value,
}

/// Returns configuration scan evidence and the independent scan request target.
///
/// # Errors
/// Returns not found for hidden systems and absent or unrelated exact scans;
/// also returns authorization, argument, cursor, or database errors.
pub(super) async fn list_scan_findings(
    pool: &PgPool,
    caller: &ToolCaller,
    arguments: Value,
) -> Result<Value, ToolError> {
    let (mut tx, environments) = scoped_read(pool, caller).await?;
    let args: ScanArgs = parse_arguments(arguments)?;
    let system = sqlx::query_as::<_, SystemMatch>(&format!(
        "{SCOPED_SYSTEMS} SELECT * FROM scoped WHERE system_id=$2"
    ))
    .bind(&environments)
    .bind(args.system_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(db_error)?
    .ok_or(ToolError::NotFound)?;

    // Only position zero in a ready snapshot identifies HEAD. A build of an
    // earlier commit is not a substitute, even if managed scan admission would
    // select that build. Exact execution reuse is proved by the existing view.
    // The view returns an execution ID even before a build exists, so use its
    // execution status and actual output to decide whether HEAD is usable.
    let head = sqlx::query_as::<_, Head>(
        r#"SELECT f.snapshot_ready_at IS NOT NULL AS snapshot_ready,f.branch,
                  c.id AS commit_id,c.git_commit_hash AS commit_hash,
                  d.id AS configuration_derivation_id,d.execution_derivation_id,
                  COALESCE(status.name IN ('build-complete','complete')
                    AND NULLIF(BTRIM(d.derivation_path),'') IS NOT NULL
                    AND NULLIF(BTRIM(d.store_path),'') IS NOT NULL,false) AS target_ready
           FROM flakes f
           LEFT JOIN flake_branch_commit_snapshot h ON h.flake_id=f.id AND h.position=0
             AND f.snapshot_ready_at IS NOT NULL
           LEFT JOIN commits c ON c.id=h.commit_id AND c.flake_id=f.id
            LEFT JOIN view_derivation_execution_evidence d ON d.commit_id=c.id
              AND d.derivation_type='nixos' AND d.derivation_name=$2
              AND lower(COALESCE(d.policy_results->>'execution_mode','native')) <> 'mock'
            LEFT JOIN derivation_statuses status ON status.id=d.execution_status_id
           WHERE f.id=$1"#,
    )
    .bind(system.flake_id)
    .bind(&system.configuration_name)
    .fetch_optional(&mut *tx)
    .await
    .map_err(db_error)?;
    let execution_id = head
        .as_ref()
        .filter(|head| head.target_ready)
        .and_then(|head| head.execution_derivation_id);
    let configuration_id = head
        .as_ref()
        .and_then(|head| head.configuration_derivation_id);

    // An explicit ID may name historical (including archived) evidence. The
    // scan's own derivation must belong to this exact flake and configuration.
    // For HEAD the same membership check applies to the proven execution id;
    // equal hashes, paths, and physical alias IDs alone cannot select a scan.
    // Keep the exact-ID primary key lookup separate from the HEAD lookup on
    // derivation and completion time. Neither branch searches unrelated scans.
    // For provenance, use the scan's own built output, never an expected path
    // or the output of a different execution adopted by that derivation.
    let selector = if args.scan_id.is_some() {
        "s.id=$1"
    } else {
        "s.derivation_id=$1 ORDER BY s.completed_at DESC,s.id DESC"
    };
    let sql = format!(
        r#"SELECT s.id AS scan_id,s.completed_at,s.scanner_name,s.scanner_version,
                  d.id AS derivation_id,c.id AS commit_id,c.git_commit_hash AS commit_hash,
                  d.derivation_path,NULLIF(BTRIM(d.store_path),'') AS scanned_system_store_path,
                  jsonb_build_object('total_packages',s.total_packages,
                    'total_vulnerabilities',s.total_vulnerabilities,'critical',s.critical_count,
                    'high',s.high_count,'medium',s.medium_count,'low',s.low_count) AS raw_counts
           FROM cve_scans s JOIN derivations d ON d.id=s.derivation_id
           JOIN commits c ON c.id=d.commit_id
            WHERE c.flake_id=$2 AND d.derivation_type='nixos' AND d.derivation_name=$3
              AND lower(COALESCE(d.policy_results->>'execution_mode','native')) <> 'mock'
              AND s.status='completed' AND s.completed_at IS NOT NULL AND s.evidence_schema_version=1
              AND {selector} LIMIT 1"#,
    );
    let selection = sqlx::query_as::<_, SealedScan>(&sql);
    let scan = match (args.scan_id, execution_id) {
        (Some(id), _) => Some(selection.bind(id)),
        (None, Some(id)) => Some(selection.bind(id)),
        (None, None) => None,
    };
    let scan = match scan {
        Some(selection) => selection
            .bind(system.flake_id)
            .bind(&system.configuration_name)
            .fetch_optional(&mut *tx)
            .await
            .map_err(db_error)?,
        None => None,
    };
    if args.scan_id.is_some() && scan.is_none() {
        return Err(ToolError::NotFound);
    }

    let package = args
        .package
        .as_deref()
        .map(|name| bounded_name(name, "package"))
        .transpose()?;
    let page = inventory_request(
        page_limit(args.limit)? as u16,
        args.cursor,
        None,
        &severity_names(args.severity)?,
    )?
    .for_configuration_scan(args.scan_id.is_some(), package);

    let attempt = match execution_id.filter(|_| args.scan_id.is_none()) {
        Some(id) => cves::newest_system_cve_attempt_tx(&mut tx, id)
            .await
            .map_err(ToolError::Internal)?,
        None => None,
    };
    let request_target = cve_scans::resolve_system_cve_scan_target(&mut *tx, system.system_id)
        .await
        .map_err(ToolError::Internal)?
        .map(|target| {
            json!({
                "expected_derivation_id": target.derivation_id,
                "matches_head": Some(target.derivation_id) == configuration_id,
                "blocked_reason": target.blocked_reason,
            })
        });

    let mut is_runtime_authoritative = false;
    let mut evidence_representation = None;
    let (state, provenance, counts, findings, next_cursor) = if let Some(scan) = scan {
        let source = SystemCveInventorySource {
            scan_id: scan.scan_id,
            completed_at: scan.completed_at,
            scanner_name: scan.scanner_name,
            scanner_version: scan.scanner_version,
        };
        let carrier = if args.scan_id.is_some() {
            scan.derivation_id
        } else {
            configuration_id.ok_or_else(|| {
                ToolError::Internal(anyhow::anyhow!("HEAD scan without configuration carrier"))
            })?
        };
        let inventory = cves::fetch_configuration_scan_page_tx(
            &mut tx,
            system.system_id,
            carrier,
            source.clone(),
            &page,
        )
        .await
        .map_err(inventory_error)?;
        evidence_representation = inventory.evidence_representation;
        // The configuration path and commit do not establish Current authority.
        // Only an exact scan match in the runtime view can set this flag.
        is_runtime_authoritative = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM view_current_cve_authority \
             WHERE system_id=$1 AND scan_id=$2)",
        )
        .bind(system.system_id)
        .bind(scan.scan_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(db_error)?;
        let findings = inventory
            .rows
            .iter()
            .map(|row| {
                json!({
                    "cve_id": row.cve_id,
                    "canonical_package_name": row.canonical_package_name,
                    "package_name": row.package_name,
                    "installed_version": row.installed_version,
                    "fixed_version": row.fixed_version,
                    "severity": row.severity,
                    "cvss_score": row.cvss_score,
                    "description": sanitize_description(&row.description),
                    "status": row.status,
                    "occurrence_derivation_path": row.occurrence_derivation_path,
                })
            })
            .collect::<Vec<_>>();
        (
            "completed",
            json!({"scan": source, "evidence_schema_version": 1,
                "scan_derivation_id": scan.derivation_id, "commit_id": scan.commit_id,
                "commit_hash": scan.commit_hash, "derivation_path": scan.derivation_path,
                "scanned_system_store_path": scan.scanned_system_store_path}),
            json!({"projected_unfiltered": inventory.unfiltered_findings,
                "projected_filtered": inventory.metadata.total_findings,
                "page": findings.len(), "raw_scanner": scan.raw_counts}),
            findings,
            inventory.next_cursor,
        )
    } else {
        page.require_first_page_without_source()
            .map_err(inventory_error)?;
        let state = if head.as_ref().and_then(|head| head.commit_id).is_none() {
            "head_not_ready"
        } else if execution_id.is_none() {
            "target_not_ready"
        } else if attempt.as_ref().is_some_and(|attempt| {
            matches!(
                attempt.status.as_str(),
                "pending" | "awaiting_build" | "awaiting_closure" | "in_progress"
            )
        }) {
            "scan_pending_for_head"
        } else {
            "no_scan_for_head"
        };
        (state, Value::Null, Value::Null, Vec::new(), None)
    };
    tx.commit().await.map_err(db_error)?;
    Ok(json!({
        "system": system,
        "selection": if args.scan_id.is_some() { "exact_scan" } else { "flake_head" },
        "evidence_kind": "configuration_scan",
        "evidence_representation": evidence_representation,
        "is_runtime_authoritative": is_runtime_authoritative,
        "read_only": true,
        "state": state,
        "head": head,
        "scan_request_target": request_target,
        "attempt": attempt,
        "provenance": provenance,
        "counts": counts,
        "findings": findings,
        "has_more": next_cursor.is_some(),
        "next_cursor": next_cursor,
    }))
}
