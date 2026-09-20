//! Persists and reads bounded CVE scan execution diagnostics.
//!
//! Diagnostic rows are append-only operational detail. They do not modify or
//! replace authoritative schema-1 vulnerability evidence.

use anyhow::{Result, bail};
use cf_protocol::builder::CveScanDiagnostic;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;

use crate::security::snapshot_redaction::redact_text;

/// Maximum diagnostic events accepted from one terminal remote report.
pub(crate) const MAX_DIAGNOSTIC_EVENTS: usize = 256;
/// Maximum persisted Unicode scalar count for one event line.
pub(crate) const MAX_DIAGNOSTIC_CHARS: usize = 2048;
/// Fixed maximum returned by the scan-detail API.
pub(crate) const MAX_DETAIL_EVENTS: i64 = 500;

/// One redacted diagnostic row ready for fenced persistence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PreparedScanDiagnostic {
    pub(crate) occurred_at: DateTime<Utc>,
    pub(crate) level: String,
    pub(crate) source: String,
    pub(crate) event_type: String,
    pub(crate) message: String,
    pub(crate) truncated: bool,
}

/// One persisted scan diagnostic returned to authorized clients.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ScanDiagnosticRow {
    pub(crate) id: i64,
    pub(crate) execution_id: Uuid,
    pub(crate) attempt_number: i32,
    pub(crate) occurred_at: DateTime<Utc>,
    pub(crate) level: String,
    pub(crate) source: String,
    pub(crate) event_type: String,
    pub(crate) message: String,
    pub(crate) truncated: bool,
}

/// Scan identity and bounded diagnostics used by the admin detail API.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ScanDiagnosticDetail {
    pub(crate) derivation_id: i32,
    pub(crate) hostname: String,
    pub(crate) flake_name: Option<String>,
    pub(crate) commit_hash: Option<String>,
    pub(crate) status: String,
    pub(crate) scanner_name: String,
    pub(crate) scanner_version: Option<String>,
    pub(crate) source_trigger: Option<String>,
    pub(crate) created_at: DateTime<Utc>,
    pub(crate) scheduled_at: Option<DateTime<Utc>>,
    pub(crate) started_at: Option<DateTime<Utc>>,
    pub(crate) completed_at: Option<DateTime<Utc>>,
    pub(crate) scan_duration_ms: Option<i32>,
    pub(crate) attempts: i32,
    pub(crate) total_packages: i32,
    pub(crate) total_vulnerabilities: i32,
    pub(crate) critical_count: i32,
    pub(crate) high_count: i32,
    pub(crate) medium_count: i32,
    pub(crate) low_count: i32,
    pub(crate) failure: Option<String>,
    pub(crate) wait_reason: Option<String>,
    pub(crate) build_job_id: Option<Uuid>,
    pub(crate) build_status: Option<String>,
    pub(crate) executor: Option<String>,
    pub(crate) archived_at: Option<DateTime<Utc>>,
    pub(crate) events: Vec<ScanDiagnosticRow>,
    pub(crate) truncated: bool,
}

/// Redacts, normalizes, and bounds untrusted diagnostic events.
pub(crate) fn prepare_diagnostics(values: &[CveScanDiagnostic]) -> Vec<PreparedScanDiagnostic> {
    let mut prepared = Vec::new();
    let mut omitted = values.len().saturating_sub(MAX_DIAGNOSTIC_EVENTS);
    for value in values.iter().take(MAX_DIAGNOSTIC_EVENTS) {
        let level = match value.level.as_str() {
            "warning" => "warning",
            "error" => "error",
            _ => "info",
        };
        let source = match value.source.as_str() {
            "vulnix" => "vulnix",
            "nix" => "nix",
            "builder" => "builder",
            _ => "server",
        };
        let event_type = match value.event_type.as_str() {
            "attempt_started" => "attempt_started",
            "attempt_completed" => "attempt_completed",
            "attempt_failed" => "attempt_failed",
            "result_persistence_failed" => "result_persistence_failed",
            "attempt_requeued" => "attempt_requeued",
            _ => "output",
        };
        for (index, raw_line) in value.message.lines().enumerate() {
            if prepared.len() >= MAX_DIAGNOSTIC_EVENTS {
                omitted += 1;
                break;
            }
            let redacted = redact_text(raw_line);
            let mut message = redacted
                .chars()
                .filter(|character| !character.is_control())
                .take(MAX_DIAGNOSTIC_CHARS)
                .collect::<String>();
            if message.trim().is_empty() {
                continue;
            }
            message = message.trim().to_string();
            prepared.push(PreparedScanDiagnostic {
                occurred_at: value.occurred_at,
                level: level.to_string(),
                source: source.to_string(),
                event_type: if index == 0 { event_type } else { "output" }.to_string(),
                truncated: value.truncated || redacted.chars().count() > MAX_DIAGNOSTIC_CHARS,
                message,
            });
        }
    }
    if omitted > 0 {
        let truncation = PreparedScanDiagnostic {
            occurred_at: Utc::now(),
            level: "warning".to_string(),
            source: "server".to_string(),
            event_type: "output".to_string(),
            message: format!(
                "Diagnostic output was truncated; at least {omitted} event(s) were omitted."
            ),
            truncated: true,
        };
        if prepared.len() < MAX_DIAGNOSTIC_EVENTS {
            prepared.push(truncation);
        } else if let Some(last) = prepared.last_mut() {
            *last = truncation;
        }
    }
    prepared
}

/// Appends diagnostics after proving the local execution still owns the scan.
pub(crate) async fn append_local_diagnostics_tx(
    tx: &mut Transaction<'_, Postgres>,
    scan_id: Uuid,
    execution_id: Uuid,
    diagnostics: &[PreparedScanDiagnostic],
) -> Result<()> {
    let attempt: Option<i32> = sqlx::query_scalar(
        "SELECT attempts FROM cve_scans WHERE id=$1 AND status='in_progress' AND scan_metadata->>'execution_id'=$2::uuid::text",
    )
    .bind(scan_id)
    .bind(execution_id)
    .fetch_optional(&mut **tx)
    .await?;
    let Some(attempt) = attempt else {
        bail!("stale CVE scan execution")
    };
    insert_diagnostics_tx(tx, scan_id, execution_id, attempt.max(1), diagnostics).await
}

/// Appends diagnostics after proving the remote builder and session own the scan.
pub(crate) async fn append_remote_diagnostics_tx(
    tx: &mut Transaction<'_, Postgres>,
    lease: cf_protocol::builder::CveScanLease,
    diagnostics: &[PreparedScanDiagnostic],
) -> Result<()> {
    let attempt: Option<i32> = sqlx::query_scalar(
        r#"SELECT scan.attempts FROM cve_scans scan JOIN builders builder ON builder.id=scan.lease_builder_id
           WHERE scan.id=$1 AND scan.execution_id=$2 AND scan.lease_builder_id=$3
             AND scan.lease_builder_session_id=$4 AND scan.status IN ('in_progress', 'completed')
             AND builder.current_session_id=$4 AND builder.enabled AND builder.registered
             AND builder.status='active'"#,
    )
    .bind(lease.scan_id)
    .bind(lease.execution_id)
    .bind(lease.builder_id)
    .bind(lease.builder_session_id)
    .fetch_optional(&mut **tx)
    .await?;
    let Some(attempt) = attempt else {
        bail!("stale CVE scan execution")
    };
    insert_diagnostics_tx(
        tx,
        lease.scan_id,
        lease.execution_id,
        attempt.max(1),
        diagnostics,
    )
    .await
}

async fn insert_diagnostics_tx(
    tx: &mut Transaction<'_, Postgres>,
    scan_id: Uuid,
    execution_id: Uuid,
    attempt: i32,
    diagnostics: &[PreparedScanDiagnostic],
) -> Result<()> {
    for event in diagnostics.iter().take(MAX_DIAGNOSTIC_EVENTS) {
        sqlx::query(
            r#"INSERT INTO cve_scan_diagnostic_events
               (scan_id, execution_id, attempt_number, occurred_at, level, source, event_type, message, truncated)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)
               ON CONFLICT (scan_id, execution_id, event_type)
               WHERE event_type IN ('attempt_started','attempt_completed','attempt_failed','attempt_requeued')
               DO NOTHING"#,
        )
        .bind(scan_id)
        .bind(execution_id)
        .bind(attempt)
        .bind(event.occurred_at)
        .bind(&event.level)
        .bind(&event.source)
        .bind(&event.event_type)
        .bind(&event.message)
        .bind(event.truncated)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

/// Returns bounded diagnostic detail for one exact scan.
pub(crate) async fn get_scan_diagnostics(
    pool: &sqlx::PgPool,
    scan_id: Uuid,
) -> Result<Option<ScanDiagnosticDetail>> {
    let metadata = sqlx::query(
        r#"
        SELECT scan.derivation_id, derivation.derivation_name AS hostname,
               flake.name AS flake_name, commit.git_commit_hash AS commit_hash,
               scan.status, scan.scanner_name, scan.scanner_version,
               scan.source_trigger, scan.created_at, scan.scheduled_at,
               COALESCE(
                   scan.lease_started_at,
                   (scan.scan_metadata ->> 'execution_started_at')::timestamptz
               ) AS started_at,
               scan.completed_at, scan.scan_duration_ms, scan.attempts,
               scan.total_packages, scan.total_vulnerabilities,
               scan.critical_count, scan.high_count, scan.medium_count,
               scan.low_count, scan.scan_metadata ->> 'error' AS failure,
               CASE scan.status
                   WHEN 'awaiting_build' THEN 'Build output is not available.'
                   WHEN 'awaiting_closure' THEN 'A completed cache closure is not available.'
               END AS wait_reason,
               COALESCE(builder.name,
                   CASE WHEN scan.scan_metadata ? 'execution_id' THEN 'server-local' END
               ) AS executor,
               archive.archived_at,
               related_build.id AS build_job_id,
               related_build.status AS build_status
        FROM cve_scans scan
        JOIN derivations derivation ON derivation.id = scan.derivation_id
        LEFT JOIN commits commit ON commit.id = derivation.commit_id
        LEFT JOIN flakes flake ON flake.id = commit.flake_id
        LEFT JOIN builders builder ON builder.id = scan.lease_builder_id
        LEFT JOIN cve_scan_archives archive ON archive.scan_id = scan.id
        LEFT JOIN build_jobs related_build
          ON related_build.id = scan.completed_build_job_id
        WHERE scan.id = $1
        "#,
    )
    .bind(scan_id)
    .fetch_optional(pool)
    .await?;
    let Some(metadata) = metadata else {
        return Ok(None);
    };
    let rows = sqlx::query(
        r#"SELECT id, execution_id, attempt_number, occurred_at, level, source, event_type, message, truncated
           FROM cve_scan_diagnostic_events WHERE scan_id=$1
           ORDER BY occurred_at, id LIMIT $2"#,
    )
    .bind(scan_id)
    .bind(MAX_DETAIL_EVENTS + 1)
    .fetch_all(pool)
    .await?;
    let truncated = rows.len() as i64 > MAX_DETAIL_EVENTS;
    let events = rows
        .into_iter()
        .take(MAX_DETAIL_EVENTS as usize)
        .map(|row| ScanDiagnosticRow {
            id: row.get("id"),
            execution_id: row.get("execution_id"),
            attempt_number: row.get("attempt_number"),
            occurred_at: row.get("occurred_at"),
            level: row.get("level"),
            source: row.get("source"),
            event_type: row.get("event_type"),
            message: row.get("message"),
            truncated: row.get("truncated"),
        })
        .collect();
    Ok(Some(ScanDiagnosticDetail {
        derivation_id: metadata.get("derivation_id"),
        hostname: metadata.get("hostname"),
        flake_name: metadata.get("flake_name"),
        commit_hash: metadata.get("commit_hash"),
        status: metadata.get("status"),
        scanner_name: metadata.get("scanner_name"),
        scanner_version: metadata.get("scanner_version"),
        source_trigger: crate::queries::cve_scans::present_scan_trigger(
            metadata
                .get::<Option<String>, _>("source_trigger")
                .as_deref(),
        ),
        created_at: metadata.get("created_at"),
        scheduled_at: metadata.get("scheduled_at"),
        started_at: metadata.get("started_at"),
        completed_at: metadata.get("completed_at"),
        scan_duration_ms: metadata.get("scan_duration_ms"),
        attempts: metadata.get("attempts"),
        total_packages: metadata.get("total_packages"),
        total_vulnerabilities: metadata.get("total_vulnerabilities"),
        critical_count: metadata.get("critical_count"),
        high_count: metadata.get("high_count"),
        medium_count: metadata.get("medium_count"),
        low_count: metadata.get("low_count"),
        failure: metadata
            .get::<Option<String>, _>("failure")
            .map(|value| redact_text(&value))
            .map(|value| value.chars().take(MAX_DIAGNOSTIC_CHARS).collect()),
        wait_reason: metadata.get("wait_reason"),
        build_job_id: metadata.get("build_job_id"),
        build_status: metadata.get("build_status"),
        executor: metadata.get("executor"),
        archived_at: metadata.get("archived_at"),
        events,
        truncated,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnostics_are_redacted_split_and_bounded() {
        let events = vec![CveScanDiagnostic {
            occurred_at: Utc::now(),
            level: "error".to_string(),
            source: "vulnix".to_string(),
            event_type: "output".to_string(),
            message: "Authorization: Bearer top-secret-token\nhttps://user:pass@example.test/repo?token=secret".to_string(),
            truncated: false,
        }];
        let prepared = prepare_diagnostics(&events);
        assert_eq!(prepared.len(), 2);
        let text = prepared
            .iter()
            .map(|event| event.message.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains("[REDACTED]"));
        assert!(!text.contains("top-secret-token"));
        assert!(!text.contains("user:pass"));
        assert!(!text.contains("token=secret"));
    }

    #[test]
    fn diagnostics_report_omitted_output_at_the_event_cap() {
        let event = CveScanDiagnostic {
            occurred_at: Utc::now(),
            level: "info".to_string(),
            source: "vulnix".to_string(),
            event_type: "output".to_string(),
            message: (0..=MAX_DIAGNOSTIC_EVENTS)
                .map(|index| format!("line {index}"))
                .collect::<Vec<_>>()
                .join("\n"),
            truncated: false,
        };

        let prepared = prepare_diagnostics(&[event]);
        assert_eq!(prepared.len(), MAX_DIAGNOSTIC_EVENTS);
        let last = prepared
            .last()
            .expect("the capped output should not be empty");
        assert!(last.truncated);
        assert!(last.message.contains("omitted"));
    }

    #[test]
    fn captured_process_output_is_redacted_and_persistence_bounded() {
        let events = vec![CveScanDiagnostic {
            occurred_at: Utc::now(),
            level: "error".to_string(),
            source: "vulnix".to_string(),
            event_type: "output".to_string(),
            message: format!("Authorization: Bearer timeout-secret\n{}", "x".repeat(4096)),
            truncated: false,
        }];

        let prepared = prepare_diagnostics(&events);
        assert_eq!(prepared[0].message, "Authorization: [REDACTED]");
        assert!(
            !prepared
                .iter()
                .any(|event| event.message.contains("timeout-secret"))
        );
        assert!(
            prepared
                .iter()
                .all(|event| event.message.chars().count() <= MAX_DIAGNOSTIC_CHARS)
        );
        assert!(prepared[1].truncated);
    }
}
