//! Reads sealed configuration evidence and checks its independent runtime authority.
//!
//! Tools: `find_systems`, `list_scan_findings` and `list_system_scans`. All
//! three need `cves:read` and read in one REPEATABLE READ snapshot that
//! [`scoped_read`] opens after it rechecks the owner and token.

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::api::models::SystemCveInventorySource;
use crate::auth::authorization::CredentialIdentity;
use crate::queries::{access_tokens, cve_scans, cves};

use super::tools::{
    MAX_CURSOR_BYTES, Severity, ToolCaller, ToolError, cursor_schema, inventory_error,
    inventory_request, limit_schema, page_limit, parse_arguments, sanitize_description,
    severity_names, severity_schema, system_id_schema,
};

const MAX_NAME_CHARS: usize = 200;
const SCOPES: &[&str] = &["cves:read"];
/// Shortest accepted commit prefix. Seven hex digits is the Git default
/// abbreviation; shorter prefixes match too many commits to be useful.
const MIN_COMMIT_PREFIX: usize = 7;
/// Length of a full SHA-1 Git commit hash in hex digits.
const MAX_COMMIT_CHARS: usize = 40;

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

/// Returns the sealed scan listing schema.
pub(super) fn list_system_scans_schema() -> Value {
    json!({
        "type": "object", "additionalProperties": false, "required": ["system_id"],
        "properties": {
            "system_id": system_id_schema(),
            "commit": {"type": "string", "minLength": MIN_COMMIT_PREFIX,
                "maxLength": MAX_COMMIT_CHARS, "pattern": "^[0-9A-Fa-f]+$",
                "description": "Full commit hash or a prefix of at least 7 hex digits."},
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
    let head = sqlx::query_as::<_, Head>("SELECT * FROM configuration_cve_head($1,$2)")
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
    // The HEAD query is also used by POA&M admission so read and write cannot
    // disagree about which scan is eligible for a planning baseline.
    let scan = if let Some(scan_id) = args.scan_id {
        sqlx::query_as::<_, SealedScan>(
            "SELECT * FROM sealed_configuration_cve_scans \
             WHERE scan_id=$1 AND flake_id=$2 AND configuration_name=$3",
        )
        .bind(scan_id)
        .bind(system.flake_id)
        .bind(&system.configuration_name)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db_error)?
    } else {
        sqlx::query_as::<_, SealedScan>("SELECT * FROM configuration_head_cve_scan($1,$2)")
            .bind(system.flake_id)
            .bind(&system.configuration_name)
            .fetch_optional(&mut *tx)
            .await
            .map_err(db_error)?
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

    // In flake_head mode, name the newest sealed scan so that an agent can
    // fall back to it with scan_id when HEAD has no sealed scan. The
    // reference never changes state, selection, findings, or planning
    // eligibility: only sealed HEAD evidence can be a planning baseline.
    // is_head has the list_system_scans meaning. Because this is the newest
    // sealed scan, is_head true also means that it is the selected scan.
    let latest_sealed_scan = if args.scan_id.is_none() {
        sealed_scans(&mut tx, &system, None, None, 1)
            .await?
            .first()
            .map(|row| {
                json!({
                    "scan_id": row.scan_id,
                    "commit_hash": row.commit_hash,
                    "completed_at": row.completed_at,
                    "is_head": execution_id == Some(row.derivation_id),
                })
            })
    } else {
        None
    };

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
        "latest_sealed_scan": latest_sealed_scan,
        "provenance": provenance,
        "counts": counts,
        "findings": findings,
        "has_more": next_cursor.is_some(),
        "next_cursor": next_cursor,
    }))
}

/// Identifies one sealed configuration scan of a system.
#[derive(sqlx::FromRow)]
struct SealedScanSummary {
    scan_id: Uuid,
    completed_at: DateTime<Utc>,
    scanner_name: String,
    scanner_version: Option<String>,
    derivation_id: i32,
    commit_id: i32,
    commit_hash: String,
    raw_counts: Value,
}

/// Returns the system's sealed configuration scans, newest first.
///
/// INVARIANT: The filter is the relation that `list_scan_findings` applies
/// to an explicit `scan_id` (same view, flake and configuration name), so
/// every returned scan ID is accepted there. A system without a flake has no
/// scans because `flake_id = NULL` matches no row.
///
/// `commit_prefix` must be validated lowercase hex; it is used in `LIKE`.
/// `after` is the exclusive keyset position `(completed_at, scan_id)`.
async fn sealed_scans(
    tx: &mut Transaction<'_, Postgres>,
    system: &SystemMatch,
    commit_prefix: Option<&str>,
    after: Option<(DateTime<Utc>, Uuid)>,
    limit: i64,
) -> Result<Vec<SealedScanSummary>, ToolError> {
    sqlx::query_as::<_, SealedScanSummary>(
        r#"SELECT scan_id,completed_at,scanner_name::text AS scanner_name,
                  scanner_version::text AS scanner_version,derivation_id,commit_id,
                  commit_hash::text AS commit_hash,raw_counts
           FROM sealed_configuration_cve_scans
           WHERE flake_id=$1 AND configuration_name=$2
             AND ($3::text IS NULL OR commit_hash::text LIKE $3 || '%')
             AND ($4::timestamptz IS NULL OR (completed_at,scan_id) < ($4,$5::uuid))
           ORDER BY completed_at DESC,scan_id DESC LIMIT $6"#,
    )
    .bind(system.flake_id)
    .bind(&system.configuration_name)
    .bind(commit_prefix)
    .bind(after.map(|(time, _)| time))
    .bind(after.map(|(_, id)| id))
    .bind(limit)
    .fetch_all(&mut **tx)
    .await
    .map_err(db_error)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ListScansArgs {
    system_id: Uuid,
    commit: Option<String>,
    cursor: Option<String>,
    limit: Option<i64>,
}

/// Keyset continuation of `list_system_scans`, bound to its filters.
#[derive(Serialize, Deserialize)]
struct ScanCursor {
    filter: String,
    completed_at: DateTime<Utc>,
    scan_id: Uuid,
}

/// Fingerprints the system and commit filter of one listing. The commit is
/// length-prefixed, so an absent filter and an empty value never collide.
fn scan_filter(system_id: Uuid, commit: Option<&str>) -> String {
    let mut digest = Sha256::new();
    digest.update(b"list_system_scans");
    digest.update(system_id.as_bytes());
    match commit {
        None => digest.update([0u8]),
        Some(text) => {
            digest.update([1u8]);
            digest.update((text.len() as u64).to_be_bytes());
            digest.update(text.as_bytes());
        }
    }
    hex::encode(digest.finalize())
}

fn decode_scan_cursor(value: &str, filter: &str) -> Result<ScanCursor, ToolError> {
    let invalid = || ToolError::InvalidArguments("cursor is invalid".into());
    if value.is_empty() || value.len() > MAX_CURSOR_BYTES {
        return Err(invalid());
    }
    let bytes = URL_SAFE_NO_PAD.decode(value).map_err(|_| invalid())?;
    let cursor: ScanCursor = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
    if cursor.filter != filter {
        return Err(ToolError::InvalidArguments(
            "cursor was issued for a different system or commit filter".into(),
        ));
    }
    Ok(cursor)
}

/// Returns a lowercase commit filter, or an argument error.
///
/// The rule matches the published JSON Schema (7 to 40 characters, pattern
/// `^[0-9A-Fa-f]+$`) exactly. Whitespace is not trimmed: a value with any
/// whitespace is rejected, so runtime and schema validation agree.
fn commit_prefix(value: &str) -> Result<String, ToolError> {
    if !(MIN_COMMIT_PREFIX..=MAX_COMMIT_CHARS).contains(&value.len())
        || !value.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(ToolError::InvalidArguments(format!(
            "commit must contain {MIN_COMMIT_PREFIX} to {MAX_COMMIT_CHARS} hex digits"
        )));
    }
    Ok(value.to_ascii_lowercase())
}

/// Pages the sealed configuration scans of one visible system, newest first.
///
/// A sealed scan is a completed, non-mock, evidence schema 1 scan of a NixOS
/// derivation of the system's flake and effective configuration name. Each
/// item reports `is_head` and `is_runtime_authoritative`.
///
/// `is_head` is evidence identity, not selection: it is true for every scan
/// whose derivation is HEAD's proven execution derivation (the
/// `execution_derivation_id` of `configuration_cve_head` when its target is
/// ready). Several scans can be HEAD evidence. `flake_head` selection uses
/// the newest of them with the same `(completed_at, scan_id)` order as this
/// list, so the selected scan is the first listed scan with `is_head` true.
/// With execution reuse, a HEAD scan can belong to an earlier commit.
///
/// `is_runtime_authoritative` is true only when `view_current_cve_authority`
/// names the scan as the system's Current authority scan, which can be a
/// historical scan when the system runs an older output. Configuration
/// evidence is never runtime evidence by itself.
///
/// Failed, pending, legacy (pre-schema-1) and mock scans are not listed.
///
/// # Errors
/// - [`ToolError::Forbidden`] when the token or owner no longer grants
///   `cves:read`.
/// - [`ToolError::NotFound`] when the system is absent or not readable.
/// - [`ToolError::InvalidArguments`] for a malformed commit, limit, or cursor,
///   or a cursor issued for other arguments.
/// - [`ToolError::Internal`] on a database failure.
pub(super) async fn list_system_scans(
    pool: &PgPool,
    caller: &ToolCaller,
    arguments: Value,
) -> Result<Value, ToolError> {
    let (mut tx, environments) = scoped_read(pool, caller).await?;
    let args: ListScansArgs = parse_arguments(arguments)?;
    let limit = page_limit(args.limit)?;
    let commit = args.commit.as_deref().map(commit_prefix).transpose()?;
    let filter = scan_filter(args.system_id, commit.as_deref());
    let after = args
        .cursor
        .as_deref()
        .map(|value| decode_scan_cursor(value, &filter))
        .transpose()?
        .map(|cursor| (cursor.completed_at, cursor.scan_id));
    let system = sqlx::query_as::<_, SystemMatch>(&format!(
        "{SCOPED_SYSTEMS} SELECT * FROM scoped WHERE system_id=$2"
    ))
    .bind(&environments)
    .bind(args.system_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(db_error)?
    .ok_or(ToolError::NotFound)?;

    let head = sqlx::query_as::<_, Head>("SELECT * FROM configuration_cve_head($1,$2)")
        .bind(system.flake_id)
        .bind(&system.configuration_name)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db_error)?;
    let head_execution = head
        .as_ref()
        .filter(|head| head.target_ready)
        .and_then(|head| head.execution_derivation_id);
    // PERFORMANCE: The authority view is read once per page, not per row.
    let authority: Vec<Uuid> = sqlx::query_scalar(
        "SELECT scan_id FROM view_current_cve_authority \
         WHERE system_id=$1 AND scan_id IS NOT NULL",
    )
    .bind(system.system_id)
    .fetch_all(&mut *tx)
    .await
    .map_err(db_error)?;

    let mut rows = sealed_scans(&mut tx, &system, commit.as_deref(), after, limit + 1).await?;
    tx.commit().await.map_err(db_error)?;
    let has_more = rows.len() > limit as usize;
    rows.truncate(limit as usize);
    let next_cursor = match rows.last().filter(|_| has_more) {
        Some(last) => Some(
            serde_json::to_vec(&ScanCursor {
                filter,
                completed_at: last.completed_at,
                scan_id: last.scan_id,
            })
            .map(|bytes| URL_SAFE_NO_PAD.encode(bytes))
            .map_err(|error| ToolError::Internal(error.into()))?,
        ),
        None => None,
    };
    let scans = rows
        .iter()
        .map(|row| {
            json!({
                "scan_id": row.scan_id,
                "commit_id": row.commit_id,
                "commit_hash": row.commit_hash,
                "derivation_id": row.derivation_id,
                "completed_at": row.completed_at,
                "scanner": {"name": row.scanner_name, "version": row.scanner_version},
                "counts": row.raw_counts,
                "is_head": head_execution == Some(row.derivation_id),
                "is_runtime_authoritative": authority.contains(&row.scan_id),
            })
        })
        .collect::<Vec<_>>();
    Ok(json!({
        "system": system,
        "evidence_kind": "configuration_scan",
        "head_commit_hash": head.and_then(|head| head.commit_hash),
        "scans": scans,
        "has_more": has_more,
        "next_cursor": next_cursor,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commit_filter_accepts_hex_prefixes_only() {
        assert_eq!(commit_prefix("72C4C5F").unwrap(), "72c4c5f");
        assert!(commit_prefix(&"a".repeat(40)).is_ok());
        for bad in [
            "72c4c5",
            "72c4c5g",
            "72c4c5%",
            &"a".repeat(41),
            " 72c4c5f",
            "72c4c5f ",
            "72c4 c5f",
            "72c4c5f\n",
            "\t72c4c5f",
        ] {
            assert!(commit_prefix(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn scan_cursor_is_bound_to_system_and_commit() {
        let system = Uuid::new_v4();
        let filter = scan_filter(system, Some("72c4c5f"));
        let cursor = URL_SAFE_NO_PAD.encode(
            serde_json::to_vec(&ScanCursor {
                filter: filter.clone(),
                completed_at: Utc::now(),
                scan_id: Uuid::new_v4(),
            })
            .unwrap(),
        );
        assert!(decode_scan_cursor(&cursor, &filter).is_ok());
        assert!(decode_scan_cursor(&cursor, &scan_filter(system, None)).is_err());
        assert!(
            decode_scan_cursor(&cursor, &scan_filter(Uuid::new_v4(), Some("72c4c5f"))).is_err()
        );
        assert!(decode_scan_cursor("not base64!", &filter).is_err());
    }
}
