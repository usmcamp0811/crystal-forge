//! Defines the MCP scan tools: `request_cve_scan` (scope `scans:request`)
//! and `get_scan_result` (scope `cves:read`).
//!
//! The dispatcher in [`super::tools`] checks the tool scope and computes the
//! effective environments before it calls a function here. The functions
//! receive that set as `effective` and never widen it.
//!
//! `request_cve_scan` uses the same admission as the browser request
//! (`crate::services::cve_scans::request_system_cve_scan`). That admission
//! repeats the owner, token and environment checks inside its writer
//! transaction, so a token revoked or an owner demoted after the dispatcher
//! check cannot admit a scan. The human owner is the audit actor; the audit
//! event also records the token.
//!
//! A hidden system and a nonexistent system return [`ToolError::NotFound`].
//! Responses contain only documented scan fields. They never include
//! `scan_metadata`, which can hold store paths and scanner error text.

use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

use super::tools::{ToolCaller, ToolError, authorized_system, parse_arguments};
use crate::models::cve_scans::{CveScan, ScanStatus};
use crate::queries::cve_scans::{
    ScanRequestToken, SystemScanAdmission, get_scan_by_id, resolve_system_cve_scan_target,
};
use crate::queries::cves::fetch_current_cve_authority_target;

use crate::services::cve_scans::{SystemScanRequestError, request_system_cve_scan};

/// Tells the agent that an accepted request is not evidence.
const ACCEPTED_NOTE: &str = "Acceptance is not a result; poll get_scan_result.";

fn system_id_schema() -> Value {
    json!({"type": "string", "format": "uuid", "description": "System ID."})
}

/// Returns the JSON Schema (draft 2020-12) of the `request_cve_scan`
/// arguments.
pub fn request_cve_scan_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "system_id": system_id_schema(),
            "expected_derivation_id": {
                "type": "integer",
                "minimum": 1,
                "description": "Scan target derivation that you expect the \
    server to select. When the server selects another target, the call fails \
    with scan_target_changed and names the current derivation_id."
            }
        },
        "required": ["system_id", "expected_derivation_id"],
        "additionalProperties": false
    })
}

/// Returns the JSON Schema (draft 2020-12) of the `get_scan_result`
/// arguments.
pub fn get_scan_result_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "system_id": system_id_schema(),
            "scan_id": {"type": "string", "format": "uuid", "description": "Scan ID."}
        },
        "required": ["system_id", "scan_id"],
        "additionalProperties": false
    })
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RequestScanArgs {
    system_id: Uuid,
    expected_derivation_id: i32,
}

/// Requests a CVE scan of one system's current scan target for the token
/// owner and returns the accepted scan.
///
/// The admission is the browser admission with the caller's token: owner row,
/// roles and memberships, and the token row are locked and checked in one
/// transaction; the owner must be an Operator with membership or an Admin; the
/// token must grant `scans:request` and the system's environment, which must
/// also be in `effective`. A repeated request for the same derivation returns
/// the active scan (existing deduplication), so the tool needs no idempotency
/// key. The audit event names the owner as actor and records the token.
///
/// The result is `{status: "accepted", scan_id, derivation_id,
/// configuration_derivation_id, note}`. `derivation_id` is the scanned
/// artifact, which can be a reused execution source of
/// `configuration_derivation_id`. Acceptance is not a scan result.
///
/// # Errors
/// - [`ToolError::InvalidArguments`] for malformed arguments, and with a
///   message that starts with `scan_target_changed`, `scan_ineligible` or
///   `scan_executor_unavailable` for the matching admission outcome.
/// - [`ToolError::NotFound`] when the system is absent, hidden, outside
///   `effective`, or the owner or token no longer authorizes the request
///   (including a token revoked or expired since the dispatcher check).
/// - [`ToolError::Internal`] when the admission or the start of a local scan
///   fails.
pub async fn request_cve_scan(
    pool: &PgPool,
    caller: &ToolCaller,
    effective: &[Uuid],
    arguments: Value,
) -> Result<Value, ToolError> {
    let args: RequestScanArgs = parse_arguments(arguments)?;
    if effective.is_empty() {
        return Err(ToolError::NotFound);
    }
    let token = ScanRequestToken {
        id: caller.token_id,
        effective_environment_ids: effective.to_vec(),
    };
    let admission = request_system_cve_scan(
        pool,
        caller.owner_user_id,
        args.system_id,
        Some(args.expected_derivation_id),
        None,
        Some(&token),
    )
    .await
    .map_err(|error| match error {
        SystemScanRequestError::Admission(error) => ToolError::Internal(error),
        SystemScanRequestError::Start(error) => {
            ToolError::Internal(anyhow::anyhow!("admitted CVE scan did not start: {error}"))
        }
    })?;
    match admission {
        SystemScanAdmission::Admitted {
            scan_id,
            target,
            configuration_derivation_id,
            ..
        } => Ok(json!({
            "status": "accepted",
            "scan_id": scan_id,
            "derivation_id": target.derivation_id,
            "configuration_derivation_id": configuration_derivation_id,
            "note": ACCEPTED_NOTE,
        })),
        SystemScanAdmission::NotFound => Err(ToolError::NotFound),
        SystemScanAdmission::TargetChanged => {
            // The admission returns TargetChanged only after the owner and
            // token checks passed, so the current target of this visible
            // system may be reported. It is read without locks and is a hint.
            let current = resolve_system_cve_scan_target(pool, args.system_id)
                .await
                .map_err(ToolError::Internal)?
                .map(|target| target.derivation_id);
            let hint = current.map_or_else(
                || " The system has no scan target now.".to_owned(),
                |id| format!(" The current scan target is derivation_id {id}."),
            );
            Err(ToolError::InvalidArguments(format!(
                "scan_target_changed: The selected configuration artifact changed.{hint} \
                 Re-read get_system_security_context before requesting a scan again."
            )))
        }
        SystemScanAdmission::Ineligible(reason) => Err(ToolError::InvalidArguments(format!(
            "scan_ineligible: {reason}"
        ))),
        SystemScanAdmission::Unavailable { scan_id, .. } => {
            Err(ToolError::InvalidArguments(format!(
                "scan_executor_unavailable: No local scanner or eligible producing builder \
                 session is available. The failed scan {scan_id} is retained."
            )))
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ScanResultArgs {
    system_id: Uuid,
    scan_id: Uuid,
}

fn status_name(status: ScanStatus) -> &'static str {
    match status {
        ScanStatus::AwaitingBuild => "awaiting_build",
        ScanStatus::AwaitingClosure => "awaiting_closure",
        ScanStatus::Pending => "pending",
        ScanStatus::InProgress => "in_progress",
        ScanStatus::Completed => "completed",
        ScanStatus::Failed => "failed",
    }
}

/// Returns a short server-written metadata value, or `None`.
///
/// SECURITY: Only the named keys are read. `terminal_reason` is a fixed reason
/// code and `execution_started_at` is a timestamp; error text and paths in
/// other keys are never returned.
fn metadata_text<'a>(scan: &'a CveScan, key: &str) -> Option<&'a str> {
    scan.scan_metadata.as_ref()?.get(key)?.as_str()
}

/// Returns the status, timestamps and counts of one scan of a visible system.
///
/// The scan must be the system's current CVE authority scan, or a scan of the
/// system's current scan target: the derivation that
/// [`resolve_system_cve_scan_target`] selects or its execution source, which
/// is what `request_cve_scan` scans. Any other scan returns not found, so a
/// caller cannot read scans of other systems through a visible system.
///
/// Severity counts are reported only for a completed scan and are `null`
/// otherwise. `started_at` is the local execution start when the scan
/// recorded one.
///
/// # Errors
/// - [`ToolError::InvalidArguments`] for malformed arguments.
/// - [`ToolError::NotFound`] when the system is absent or outside `effective`,
///   or the scan is absent or not related to the system as described above.
/// - [`ToolError::Internal`] on a database failure.
pub async fn get_scan_result(
    pool: &PgPool,
    _caller: &ToolCaller,
    effective: &[Uuid],
    arguments: Value,
) -> Result<Value, ToolError> {
    let args: ScanResultArgs = parse_arguments(arguments)?;
    let system = authorized_system(pool, effective, args.system_id).await?;
    let scan = get_scan_by_id(pool, args.scan_id)
        .await
        .map_err(ToolError::Internal)?
        .ok_or(ToolError::NotFound)?;
    let authority_scan = fetch_current_cve_authority_target(pool, system.id)
        .await
        .map_err(ToolError::Internal)?
        .map(|target| target.scan_id);
    let related = if authority_scan == Some(scan.id) {
        true
    } else {
        match resolve_system_cve_scan_target(pool, system.id)
            .await
            .map_err(ToolError::Internal)?
        {
            Some(target) => {
                // INVARIANT: Admission scans the execution source of the
                // selected configuration derivation, so both ids identify
                // the current target.
                let source: i32 = sqlx::query_scalar("SELECT derivation_execution_source($1)")
                    .bind(target.derivation_id)
                    .fetch_one(pool)
                    .await
                    .map_err(|error| ToolError::Internal(error.into()))?;
                scan.derivation_id == target.derivation_id || scan.derivation_id == source
            }
            None => false,
        }
    };
    if !related {
        return Err(ToolError::NotFound);
    }
    let started_at = metadata_text(&scan, "execution_started_at")
        .and_then(|text| DateTime::parse_from_rfc3339(text).ok())
        .map(|time| time.with_timezone(&Utc));
    let completed = scan.status == ScanStatus::Completed;
    let counts = completed.then(|| {
        json!({
            "total_packages": scan.total_packages,
            "total_vulnerabilities": scan.total_vulnerabilities,
            "critical": scan.critical_count,
            "high": scan.high_count,
            "medium": scan.medium_count,
            "low": scan.low_count,
        })
    });
    Ok(json!({
        "scan_id": scan.id,
        "system_id": system.id,
        "status": status_name(scan.status),
        "derivation_id": scan.derivation_id,
        "is_current_authority": authority_scan == Some(scan.id),
        "created_at": scan.created_at,
        "scheduled_at": scan.scheduled_at,
        "started_at": started_at,
        "completed_at": scan.completed_at,
        "attempts": scan.attempts,
        "terminal_reason": metadata_text(&scan, "terminal_reason"),
        "scanner": {"name": scan.scanner_name, "version": scan.scanner_version},
        "counts": counts,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schemas_are_closed_objects_with_required_inputs() {
        let request = request_cve_scan_schema();
        assert_eq!(request["additionalProperties"], false);
        assert_eq!(
            request["required"],
            json!(["system_id", "expected_derivation_id"])
        );
        let result = get_scan_result_schema();
        assert_eq!(result["additionalProperties"], false);
        assert_eq!(result["required"], json!(["system_id", "scan_id"]));
    }

    #[test]
    fn request_arguments_require_an_integer_derivation() {
        let system_id = Uuid::new_v4();
        assert!(parse_arguments::<RequestScanArgs>(json!({"system_id": system_id})).is_err());
        assert!(
            parse_arguments::<RequestScanArgs>(
                json!({"system_id": system_id, "expected_derivation_id": "7"})
            )
            .is_err()
        );
        assert!(
            parse_arguments::<RequestScanArgs>(
                json!({"system_id": system_id, "expected_derivation_id": 7, "extra": 1})
            )
            .is_err()
        );
        let args: RequestScanArgs =
            parse_arguments(json!({"system_id": system_id, "expected_derivation_id": 7}))
                .expect("valid arguments");
        assert_eq!(args.expected_derivation_id, 7);
    }
}
