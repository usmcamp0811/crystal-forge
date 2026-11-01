//! Reads configuration evidence without conferring runtime or deployment authority.
//!
//! Wire contract for `GET /api/v1/systems/:id/configuration-assurance`:
//! UUIDs are strings, timestamps are RFC 3339 strings, IDs/counts are integers.
//! Optional values serialize as JSON null, never omitted. Empty history is `[]`.
//! `selected_revision` is the ready snapshot's position-0 commit for the
//! registered flake's exact branch, not the newest successful evaluation/build.
//! `evaluation.availability` is available, removed, or unavailable. Only native
//! discovery establishes availability; missing discovery is not removal.
//! `evaluation.state` is the configuration's native outcome when present,
//! otherwise the attempt state, or unavailable. Attempt completion alone never
//! means that this configuration evaluated successfully.
//! `selected_artifact` is exact revision/configuration evidence, never runtime.
//! Publication time proves only an exact completed cache push, not current cache
//! reachability, credentials, deployment eligibility, or runtime activation.
//! `selected_scan` is the latest lifecycle for that artifact, including waiting
//! and failed states. `latest_terminal_scan` spans revisions; `latest_good_scan`
//! is the newest sealed schema-1 scan, which can be historical. Every scan carries
//! its own revision and derivation identity. Counts are null for unsealed scans.
//! Counts are distinct non-whitelisted CVE IDs in immutable scan observations;
//! severity uses current CVE catalog metadata, so totals include unknown severity.
//! Histories contain at most 20 rows per kind, newest event first with ID ties.
//! They retain old revisions when the selected configuration is removed, but do
//! not reconstruct registrations' previous flake/configuration associations.
//! Diagnostics contain at most 50 newest receipt-ordered events per scan and
//! 2048 characters per message. Errors are redacted and bounded. No logs,
//! credentials, lease tokens,
//! guest observations, POA&M changes, or Current claims belong to this response.

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, types::Json};
use uuid::Uuid;

/// Contains independently identified configuration evidence and bounded history.
#[derive(Debug, Serialize, Deserialize)]
pub struct ConfigurationAssurance {
    /// Identifies the registration, not a shared artifact owner.
    pub system_id: Uuid,
    /// Contains managed, external, or reference.
    pub management_mode: String,
    /// Contains the effective registered NixOS configuration name.
    pub configuration_name: String,
    /// Identifies the registered flake, or null when unassigned.
    pub flake_id: Option<i32>,
    /// Contains the registered branch, or null when unassigned.
    pub branch: Option<String>,
    /// Identifies the selected branch head, or null before snapshot readiness.
    pub selected_revision: Option<RevisionEvidence>,
    /// Describes selected-revision discovery and evaluation independently.
    pub evaluation: EvaluationEvidence,
    /// Contains only an exact selected-revision/configuration derivation.
    pub selected_artifact: Option<ArtifactEvidence>,
    /// Contains the newest scan lifecycle of the selected artifact.
    pub selected_scan: Option<ScanEvidence>,
    /// Contains the newest completed or failed scan across retained revisions.
    pub latest_terminal_scan: Option<ScanEvidence>,
    /// Contains the newest sealed successful scan across retained revisions.
    pub latest_good_scan: Option<ScanEvidence>,
    /// Contains bounded evidence events, including historical failures.
    pub history: EvidenceHistory,
}

/// Identifies an immutable source revision.
#[derive(Debug, Serialize, Deserialize)]
pub struct RevisionEvidence {
    /// Identifies the persisted commit.
    pub commit_id: i32,
    /// Contains the full Git revision.
    pub sha: String,
    /// Contains the Git commit date.
    pub committed_at: DateTime<Utc>,
    /// Contains the commit message when recorded.
    pub message: Option<String>,
}

/// Separates discovery availability from attempt and configuration outcomes.
#[derive(Debug, Serialize, Deserialize)]
pub struct EvaluationEvidence {
    /// Contains available, removed, or unavailable.
    pub availability: String,
    /// Contains evaluated, eval_failed, an attempt state, or unavailable.
    pub state: String,
    /// Identifies the exact native evaluation attempt when present.
    pub attempt_id: Option<Uuid>,
    /// Contains a redacted native-outcome or attempt failure, if recorded.
    pub error: Option<String>,
    /// Contains the native outcome capture or attempt completion time.
    pub completed_at: Option<DateTime<Utc>>,
}

/// Identifies an exact derivation and its actual build/publication evidence.
#[derive(Debug, Serialize, Deserialize)]
pub struct ArtifactEvidence {
    /// Identifies the derivation.
    pub derivation_id: i32,
    /// Identifies the physical origin only within the visible source scope.
    #[serde(default)]
    pub physical_derivation_id: Option<i32>,
    /// Identifies the real execution source, independently of this evaluation.
    #[serde(default)]
    pub execution_derivation_id: Option<i32>,
    /// Identifies the actual execution revision; never the selected-head guess.
    #[serde(default)]
    pub execution_revision: Option<RevisionEvidence>,
    /// Identifies the source revision.
    pub revision: RevisionEvidence,
    /// Contains the persisted derivation configuration name.
    pub configuration_name: String,
    /// Contains the derivation status name without inferring publication.
    pub state: String,
    /// Contains the derivation path, or null before evaluation produces one.
    pub derivation_path: Option<String>,
    /// Contains the actual output path, never the expected output path.
    pub output_path: Option<String>,
    /// Contains the evaluated expected output, independent of build completion.
    pub expected_output_path: Option<String>,
    /// Contains the exact completed cache-push time, or null if unpublished.
    pub published_at: Option<DateTime<Utc>>,
    /// Contains a redacted derivation error when recorded.
    pub error: Option<String>,
    /// Contains the latest persisted build attempt, independent of artifact state.
    pub build: Option<BuildEvidence>,
}

/// Identifies one real build attempt without treating success as publication.
#[derive(Debug, Serialize, Deserialize)]
pub struct BuildEvidence {
    /// Identifies the build job.
    pub job_id: Uuid,
    /// Identifies the job's immutable execution derivation, not its consumer.
    #[serde(default)]
    pub derivation_id: Option<i32>,
    /// Identifies the job's actual source revision.
    #[serde(default)]
    pub revision: Option<RevisionEvidence>,
    /// Contains the persisted job status.
    pub state: String,
    /// Contains job creation time.
    pub created_at: DateTime<Utc>,
    /// Contains execution start time, if started.
    pub started_at: Option<DateTime<Utc>>,
    /// Contains terminal time, if finished.
    pub completed_at: Option<DateTime<Utc>>,
    /// Contains the server failure code, if recorded.
    pub failure_code: Option<String>,
}

/// Counts distinct non-whitelisted CVE identities in one sealed scan.
#[derive(Debug, Serialize, Deserialize)]
pub struct EvidenceCveCounts {
    /// Counts CVEs, including catalog entries with unknown severity.
    pub total: i64,
    /// Counts catalog CRITICAL CVEs.
    pub critical: i64,
    /// Counts catalog HIGH CVEs.
    pub high: i64,
    /// Counts catalog MEDIUM CVEs.
    pub medium: i64,
    /// Counts catalog LOW CVEs.
    pub low: i64,
}

/// Identifies a scan independently of selected revision and runtime state.
#[derive(Debug, Serialize, Deserialize)]
pub struct ScanEvidence {
    /// Identifies the persisted scan lifecycle.
    pub scan_id: Uuid,
    /// Identifies the exact scanned derivation.
    pub derivation_id: i32,
    /// Identifies the scanned source revision.
    pub revision: RevisionEvidence,
    /// Contains the scanned configuration name.
    pub configuration_name: String,
    /// Contains the persisted scan status.
    pub state: String,
    /// Contains lifecycle creation time.
    pub created_at: DateTime<Utc>,
    /// Contains terminal time when recorded.
    pub completed_at: Option<DateTime<Utc>>,
    /// Contains evidence schema version; only version 1 permits counts.
    pub evidence_schema_version: i32,
    /// Contains exact sealed counts, or null for missing/unsealed evidence.
    pub cves: Option<EvidenceCveCounts>,
    /// Contains a bounded redacted scan failure, if recorded.
    pub error: Option<String>,
    /// Contains newest-first bounded persisted diagnostic events.
    pub diagnostics: Vec<DiagnosticEvidence>,
    /// Is true when more than 50 diagnostic events exist.
    pub diagnostics_truncated: bool,
}

/// Contains a redacted scan diagnostic without execution credentials.
#[derive(Debug, Serialize, Deserialize)]
pub struct DiagnosticEvidence {
    /// Contains the recorded event time.
    pub occurred_at: DateTime<Utc>,
    /// Contains the persisted event level.
    pub level: String,
    /// Contains the persisted event type.
    pub event_type: String,
    /// Contains a bounded redacted diagnostic message.
    pub message: String,
}

/// Contains real native evaluation history with exact source identity.
#[derive(Debug, Serialize, Deserialize)]
pub struct EvaluationEvent {
    /// Identifies the evaluated source revision.
    pub revision: RevisionEvidence,
    /// Contains the native configuration or attempt outcome.
    pub evaluation: EvaluationEvidence,
    /// Contains the outcome capture, completion, start, or queue time.
    pub occurred_at: DateTime<Utc>,
}

/// Contains newest-first prefixes, not an exhaustive or runtime timeline.
#[derive(Debug, Serialize, Deserialize)]
pub struct EvidenceHistory {
    /// States the fixed per-kind response limit.
    pub limit_per_kind: i32,
    /// Contains native evaluation events for this flake/configuration.
    pub evaluations: Vec<EvaluationEvent>,
    /// Contains build attempts with exact derivation/revision context.
    pub builds: Vec<ArtifactEvidence>,
    /// Contains scan lifecycles with exact derivation/revision context.
    pub scans: Vec<ScanEvidence>,
}

// INVARIANT: One SQL statement supplies a consistent MVCC snapshot. Selection,
// authorization, and history use the same registration. Historical artifacts
// survive failed/removed head configurations; they never replace head evidence.
const EVIDENCE_SQL: &str = r#"
WITH registration AS (
 SELECT s.*, COALESCE(NULLIF(BTRIM(s.system_configuration_name), ''), s.hostname) AS config_name,
        f.branch, head.commit_id AS selected_commit_id
 FROM systems s LEFT JOIN flakes f ON f.id=s.flake_id
 LEFT JOIN flake_branch_commit_snapshot head ON head.flake_id=f.id AND head.position=0
   AND f.snapshot_ready_at IS NOT NULL
 WHERE s.id=$1 AND ($2::uuid IS NULL OR EXISTS (
   SELECT 1 FROM user_environment_memberships m
   WHERE m.user_id=$2 AND m.environment_id=s.environment_id))
), revisions AS (
 SELECT c.*, jsonb_build_object('commit_id',c.id,'sha',c.git_commit_hash,
   'committed_at',c.commit_timestamp,'message',c.message) AS revision
 FROM commits c JOIN registration r ON r.flake_id=c.flake_id
), evaluations AS (
 SELECT ea.id, c.id AS commit_id, ea.attempt_number,
   COALESCE(o.captured_at,ea.completed_at,ea.started_at,ea.created_at) AS occurred_at,
   c.revision, jsonb_build_object(
     'availability', CASE WHEN i.evaluation_attempt_id IS NULL THEN 'unavailable'
       WHEN r.config_name=ANY(i.configuration_names) THEN 'available' ELSE 'removed' END,
     'state', COALESCE(o.eval_state,CASE WHEN ea.status='complete' THEN 'unavailable' ELSE ea.status END), 'attempt_id',ea.id,
     'error',COALESCE(o.evaluation_failure_error,ea.error_message),
     'completed_at',COALESCE(o.captured_at,ea.completed_at)) AS evidence
 FROM revisions c JOIN evaluation_attempts ea ON ea.commit_id=c.id
 CROSS JOIN registration r
 LEFT JOIN evaluation_attempt_inventory i ON i.evaluation_attempt_id=ea.id AND i.execution_mode='native'
 LEFT JOIN evaluation_native_outcomes o ON o.evaluation_attempt_id=ea.id AND o.configuration_name=r.config_name
 WHERE i.execution_mode='native' OR o.evaluation_attempt_id IS NOT NULL
   OR (i.evaluation_attempt_id IS NULL AND NOT EXISTS (
     SELECT 1 FROM evaluation_attempt_inventory mock WHERE mock.evaluation_attempt_id=ea.id AND mock.execution_mode='mock'))
), artifacts AS (
  SELECT d.*, c.revision, execution_revision.revision AS execution_revision, ds.name AS state,
   (SELECT MAX(p.completed_at) FROM cache_push_jobs p
     WHERE p.derivation_id=d.execution_derivation_id AND p.store_path=d.store_path AND p.status='completed') AS published_at
  FROM view_derivation_execution_evidence d JOIN revisions c ON c.id=d.commit_id
  JOIN revisions execution_revision ON execution_revision.id=d.execution_commit_id
 JOIN derivation_statuses ds ON ds.id=d.status_id CROSS JOIN registration r
 WHERE d.derivation_type='nixos' AND d.derivation_name=r.config_name
   AND lower(COALESCE(d.policy_results->>'execution_mode','native')) <> 'mock'
), builds AS (
  SELECT d.id AS derivation_id, d.commit_id,
    COALESCE(j.derivation_id,d.execution_derivation_id) AS execution_derivation_id,
    d.reused_build_job_id AS adopted_job_id,j.id AS job_id,
   COALESCE(j.completed_at,j.started_at,j.created_at) AS occurred_at,
    jsonb_build_object('derivation_id',d.id,'revision',d.revision,
      'physical_derivation_id',CASE WHEN derivations_share_evidence_scope(d.id,d.physical_derivation_id) THEN d.physical_derivation_id ELSE NULL END,
      'execution_derivation_id',CASE WHEN j.id IS NULL AND d.store_path IS NULL THEN NULL
          ELSE COALESCE(j.derivation_id,d.execution_derivation_id) END,
      'execution_revision',CASE WHEN j.id IS NULL AND d.store_path IS NULL THEN NULL
          ELSE COALESCE(job_revision.revision,d.execution_revision) END,
     'configuration_name',d.derivation_name,'state',d.state,
     'derivation_path',d.derivation_path,'output_path',CASE WHEN j.id IS NULL THEN d.store_path ELSE execution.store_path END,
     'expected_output_path',d.expected_store_path,
     'published_at',CASE WHEN j.id IS NULL THEN d.published_at ELSE (
       SELECT max(push.completed_at) FROM cache_push_jobs push
       WHERE push.derivation_id=execution.id AND push.store_path=execution.store_path AND push.status='completed') END,
     'error',d.error_message,
     'build',CASE WHEN j.id IS NULL THEN NULL ELSE jsonb_build_object(
        'job_id',j.id,'derivation_id',j.derivation_id,'revision',job_revision.revision,
        'state',j.status,'created_at',j.created_at,'started_at',j.started_at,
       'completed_at',j.completed_at,'failure_code',j.server_failure_code) END) AS evidence
  FROM artifacts d LEFT JOIN build_jobs j ON j.derivation_id=d.id OR j.id=d.reused_build_job_id
  LEFT JOIN derivations execution ON execution.id=j.derivation_id
  LEFT JOIN revisions job_revision ON job_revision.id=execution.commit_id
 ), selected_artifact AS (
 SELECT b.* FROM builds b CROSS JOIN registration r WHERE b.commit_id=r.selected_commit_id
 ORDER BY b.derivation_id DESC,(b.job_id=b.adopted_job_id) DESC NULLS LAST,
          b.occurred_at DESC NULLS LAST,b.job_id DESC LIMIT 1
), scan_candidates AS (
 (SELECT scan.id FROM cve_scans scan JOIN artifacts d ON d.id=scan.derivation_id
  ORDER BY COALESCE(scan.completed_at,scan.created_at) DESC,scan.id DESC LIMIT 20)
 UNION
  (SELECT scan.id FROM cve_scans scan JOIN selected_artifact d
     ON d.execution_derivation_id=scan.derivation_id
        OR derivations_share_evidence_scope(d.derivation_id,scan.derivation_id)
  ORDER BY scan.created_at DESC,scan.id DESC LIMIT 1)
 UNION
 (SELECT scan.id FROM cve_scans scan JOIN artifacts d ON d.id=scan.derivation_id
  WHERE scan.status IN ('completed','failed')
  ORDER BY COALESCE(scan.completed_at,scan.created_at) DESC,scan.id DESC LIMIT 1)
 UNION
 (SELECT scan.id FROM cve_scans scan JOIN artifacts d ON d.id=scan.derivation_id
  WHERE scan.status='completed' AND scan.completed_at IS NOT NULL AND scan.evidence_schema_version=1
  ORDER BY scan.completed_at DESC,scan.id DESC LIMIT 1)
), scans AS (
 SELECT scan.id, scan.derivation_id, d.commit_id, scan.status, scan.evidence_schema_version,
   scan.created_at, scan.completed_at,
   jsonb_build_object('scan_id',scan.id,'derivation_id',d.id,'revision',d.revision,
     'configuration_name',d.derivation_name,'state',scan.status,
     'created_at',scan.created_at,'completed_at',scan.completed_at,
     'evidence_schema_version',scan.evidence_schema_version,'error',scan.scan_metadata->>'error',
     'cves', CASE WHEN scan.status='completed' AND scan.completed_at IS NOT NULL
       AND scan.evidence_schema_version=1 THEN (
       SELECT jsonb_build_object('total',COUNT(DISTINCT o.canonical_cve_id),
          'critical',COUNT(DISTINCT o.canonical_cve_id) FILTER (WHERE public.severity_from_cvss(c.cvss_v3_score)='CRITICAL'),
          'high',COUNT(DISTINCT o.canonical_cve_id) FILTER (WHERE public.severity_from_cvss(c.cvss_v3_score)='HIGH'),
          'medium',COUNT(DISTINCT o.canonical_cve_id) FILTER (WHERE public.severity_from_cvss(c.cvss_v3_score)='MEDIUM'),
          'low',COUNT(DISTINCT o.canonical_cve_id) FILTER (WHERE public.severity_from_cvss(c.cvss_v3_score)='LOW'))
       FROM cve_scan_vulnerability_observations o JOIN cves c ON c.id=o.canonical_cve_id
       WHERE o.scan_id=scan.id AND NOT o.is_whitelisted) ELSE NULL END,
     'diagnostics',COALESCE((SELECT jsonb_agg(event ORDER BY event_id DESC) FROM (
       SELECT e.id AS event_id,jsonb_build_object(
         'occurred_at',e.occurred_at,'level',e.level,'event_type',e.event_type,
         'message',LEFT(e.message,2048)) AS event
       FROM cve_scan_diagnostic_events e WHERE e.scan_id=scan.id
       ORDER BY e.id DESC LIMIT 50) events),'[]'::jsonb),
     'diagnostics_truncated',(SELECT COUNT(*)>50 FROM cve_scan_diagnostic_events e WHERE e.scan_id=scan.id)
   ) AS evidence
 FROM cve_scans scan JOIN scan_candidates candidate ON candidate.id=scan.id
 JOIN artifacts d ON d.id=scan.derivation_id
)
SELECT jsonb_build_object('system_id',r.id,'management_mode',r.management_mode,
 'configuration_name',r.config_name,'flake_id',r.flake_id,'branch',r.branch,
 'selected_revision',(SELECT revision FROM revisions WHERE id=r.selected_commit_id),
 'evaluation',COALESCE((SELECT evidence FROM evaluations WHERE commit_id=r.selected_commit_id
   ORDER BY attempt_number DESC,id DESC LIMIT 1),jsonb_build_object('availability','unavailable',
     'state','unavailable','attempt_id',NULL,'error',NULL,'completed_at',NULL)),
 'selected_artifact',(SELECT evidence FROM selected_artifact),
  'selected_scan',(SELECT evidence FROM scans WHERE derivation_id=(SELECT execution_derivation_id FROM selected_artifact)
    OR derivations_share_evidence_scope((SELECT derivation_id FROM selected_artifact),derivation_id)
   ORDER BY created_at DESC,id DESC LIMIT 1),
 'latest_terminal_scan',(SELECT evidence FROM scans WHERE status IN ('completed','failed')
   ORDER BY COALESCE(completed_at,created_at) DESC,id DESC LIMIT 1),
 'latest_good_scan',(SELECT evidence FROM scans WHERE status='completed' AND completed_at IS NOT NULL
   AND evidence_schema_version=1 ORDER BY completed_at DESC,id DESC LIMIT 1),
 'history',jsonb_build_object('limit_per_kind',20,
   'evaluations',COALESCE((SELECT jsonb_agg(event ORDER BY occurred_at DESC,id DESC) FROM (
     SELECT id,occurred_at,jsonb_build_object('revision',revision,'evaluation',evidence,'occurred_at',occurred_at) AS event
     FROM evaluations ORDER BY occurred_at DESC,id DESC LIMIT 20) history),'[]'::jsonb),
   'builds',COALESCE((SELECT jsonb_agg(evidence ORDER BY occurred_at DESC,job_id DESC) FROM (
      SELECT * FROM builds WHERE job_id IS NOT NULL AND derivation_id=execution_derivation_id
      ORDER BY occurred_at DESC,job_id DESC LIMIT 20) history),'[]'::jsonb),
   'scans',COALESCE((SELECT jsonb_agg(evidence ORDER BY occurred_at DESC,id DESC) FROM (
     SELECT *,COALESCE(completed_at,created_at) AS occurred_at FROM scans
     ORDER BY COALESCE(completed_at,created_at) DESC,id DESC LIMIT 20) history),'[]'::jsonb)))
FROM registration r
"#;

fn redact_evidence(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(fields) => {
            for (key, value) in fields {
                if matches!(key.as_str(), "error" | "message") {
                    if let Some(text) = value.as_str() {
                        *value = serde_json::Value::String(
                            crate::security::snapshot_redaction::redact_text(text)
                                .chars()
                                .take(2048)
                                .collect(),
                        );
                    }
                } else {
                    redact_evidence(value);
                }
            }
        }
        serde_json::Value::Array(values) => values.iter_mut().for_each(redact_evidence),
        _ => {}
    }
}

/// Returns read-only evidence within the caller's current environment scope.
///
/// Administrators pass `None`; all other authorized roles pass their user ID.
/// Missing and inaccessible registrations both return `None`. Inactive
/// registrations retain readable history. This read performs no external work.
///
/// # Errors
/// Returns database or decoding errors without returning partial evidence.
///
/// # Examples
/// ```no_run
/// # async fn read(pool: &sqlx::PgPool, system: uuid::Uuid, viewer: uuid::Uuid) -> anyhow::Result<()> {
/// let evidence = crystal_forge::queries::configuration_assurance::fetch(
///     pool, system, Some(viewer),
/// ).await?;
/// // An inaccessible registration and a missing registration both return None.
/// # Ok(()) }
/// ```
pub async fn fetch(
    pool: &PgPool,
    system_id: Uuid,
    visibility_user_id: Option<Uuid>,
) -> Result<Option<ConfigurationAssurance>> {
    let row: Option<Json<serde_json::Value>> = sqlx::query_scalar(EVIDENCE_SQL)
        .bind(system_id)
        .bind(visibility_user_id)
        .fetch_optional(pool)
        .await?;
    row.map(|Json(mut value)| {
        redact_evidence(&mut value);
        Ok(serde_json::from_value(value)?)
    })
    .transpose()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_scan_counts_stay_null_and_diagnostics_are_bounded() {
        let mut value = serde_json::json!({"cves":null,"error":"x".repeat(3000),
            "diagnostics":[{"message":"y".repeat(3000)}]});
        redact_evidence(&mut value);
        assert!(value["cves"].is_null());
        assert_eq!(value["error"].as_str().unwrap().len(), 2048);
        assert_eq!(
            value["diagnostics"][0]["message"].as_str().unwrap().len(),
            2048
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires isolated migration-backed PostgreSQL"]
    async fn removed_head_retains_real_scan_history_without_promoting_it(pool: PgPool) {
        let flake: i32 = sqlx::query_scalar(
            "INSERT INTO flakes(name,repo_url,branch,snapshot_ready_at) VALUES ('assurance','https://example.invalid/evidence','release',NOW()) RETURNING id",
        ).fetch_one(&pool).await.unwrap();
        let system: Uuid = sqlx::query_scalar(
            "INSERT INTO systems(hostname,system_configuration_name,flake_id,management_mode,public_key,derivation,deployment_policy)
             VALUES ('baseline registration','baseline',$1,'reference',NULL,'','manual') RETURNING id",
        ).bind(flake).fetch_one(&pool).await.unwrap();
        let old: i32 = sqlx::query_scalar(
            "INSERT INTO commits(flake_id,git_commit_hash,commit_timestamp) VALUES ($1,repeat('a',40),NOW()-INTERVAL '1 day') RETURNING id",
        ).bind(flake).fetch_one(&pool).await.unwrap();
        let head: i32 = sqlx::query_scalar(
            "INSERT INTO commits(flake_id,git_commit_hash,commit_timestamp) VALUES ($1,repeat('b',40),NOW()) RETURNING id",
        ).bind(flake).fetch_one(&pool).await.unwrap();
        sqlx::query("INSERT INTO flake_branch_commit_snapshot(flake_id,commit_id,position) VALUES ($1,$2,0)")
            .bind(flake).bind(head).execute(&pool).await.unwrap();
        sqlx::query(
            "INSERT INTO evaluation_attempt_inventory(evaluation_attempt_id,commit_id,attempt_number,configuration_names,selected_configuration_names,execution_mode)
             SELECT id,commit_id,attempt_number,ARRAY[]::text[],ARRAY[]::text[],'native' FROM evaluation_attempts WHERE commit_id=$1",
        ).bind(head).execute(&pool).await.unwrap();
        let derivation: i32 = sqlx::query_scalar(
            "INSERT INTO derivations(commit_id,derivation_name,derivation_type,derivation_path,store_path,status_id)
             SELECT $1,'baseline','nixos','/nix/store/evidence.drv','/nix/store/evidence',id FROM derivation_statuses WHERE name='complete' RETURNING id",
        ).bind(old).fetch_one(&pool).await.unwrap();
        let sealed: Uuid = sqlx::query_scalar(
            "INSERT INTO cve_scans(derivation_id,scanner_name,status) VALUES ($1,'vulnix','in_progress') RETURNING id",
        ).bind(derivation).fetch_one(&pool).await.unwrap();
        sqlx::query(
            "INSERT INTO systems(hostname,system_configuration_name,flake_id,management_mode,public_key,derivation,deployment_policy)
             VALUES ('external registration','baseline',$1,'external',NULL,'','manual')",
        ).bind(flake).execute(&pool).await.unwrap();
        let uncovered = crate::queries::cves::fetch_cve_fleet_stats(
            &pool,
            &crate::queries::cves::CveReadScope::All,
        )
        .await
        .unwrap()
        .no_scan_systems;
        sqlx::query("INSERT INTO cves(id,cvss_v3_score) VALUES ('CVE-2026-50801',9.8)")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO cve_scan_vulnerability_observations(scan_id,canonical_cve_id,canonical_package_name,observed_package_name,observed_package_version,observed_derivation_path,is_whitelisted,detection_method)
             VALUES ($1,'CVE-2026-50801','pkg','pkg','1','/nix/store/pkg.drv',false,'vulnix')",
        ).bind(sealed).execute(&pool).await.unwrap();
        sqlx::query("UPDATE cve_scans SET status='completed',completed_at=NOW()-INTERVAL '1 hour',evidence_schema_version=1 WHERE id=$1")
            .bind(sealed).execute(&pool).await.unwrap();
        let covered = crate::queries::cves::fetch_cve_fleet_stats(
            &pool,
            &crate::queries::cves::CveReadScope::All,
        )
        .await
        .unwrap();
        assert_eq!(covered.no_scan_systems, uncovered - 2);
        assert_eq!(covered.current_systems_affected, 0);
        let failed: Uuid = sqlx::query_scalar(
            "INSERT INTO cve_scans(derivation_id,scanner_name,status,completed_at,scan_metadata)
             VALUES ($1,'vulnix','failed',NOW(),'{\"error\":\"scanner failed\"}') RETURNING id",
        )
        .bind(derivation)
        .fetch_one(&pool)
        .await
        .unwrap();
        let evidence = fetch(&pool, system, None).await.unwrap().unwrap();
        assert_eq!(evidence.selected_revision.unwrap().commit_id, head);
        assert_eq!(evidence.evaluation.availability, "removed");
        assert!(evidence.selected_artifact.is_none());
        assert!(evidence.selected_scan.is_none());
        let terminal = evidence.latest_terminal_scan.unwrap();
        assert_eq!(terminal.scan_id, failed);
        assert_eq!(terminal.error.as_deref(), Some("scanner failed"));
        assert!(terminal.cves.is_none());
        let good = evidence.latest_good_scan.unwrap();
        assert_eq!(good.scan_id, sealed);
        assert_eq!(good.revision.commit_id, old);
        let counts = good.cves.unwrap();
        assert_eq!(counts.total, 1);
        assert_eq!(counts.critical, 1);
        assert_eq!(counts.high + counts.medium + counts.low, 0);
        assert_eq!(evidence.history.scans.len(), 2);
        // A viewer without membership cannot infer even registration existence.
        assert!(
            fetch(&pool, system, Some(Uuid::new_v4()))
                .await
                .unwrap()
                .is_none()
        );
        let head_derivation: i32 = sqlx::query_scalar(
            "INSERT INTO derivations(commit_id,derivation_name,derivation_type,status_id,error_message)
             SELECT $1,'baseline','nixos',id,'build failed' FROM derivation_statuses WHERE name='build-failed' RETURNING id",
        ).bind(head).fetch_one(&pool).await.unwrap();
        sqlx::query(
            "INSERT INTO cve_scans(derivation_id,scanner_name,status,completed_at,created_at)
             SELECT $1,'vulnix','failed',NOW()+n*INTERVAL '1 second',NOW()+n*INTERVAL '1 second'
             FROM generate_series(1,25) n",
        )
        .bind(head_derivation)
        .execute(&pool)
        .await
        .unwrap();
        let evidence = fetch(&pool, system, None).await.unwrap().unwrap();
        assert_eq!(
            evidence.selected_artifact.unwrap().derivation_id,
            head_derivation
        );
        assert!(evidence.selected_scan.unwrap().cves.is_none());
        assert_eq!(evidence.latest_good_scan.unwrap().scan_id, sealed);
        assert_eq!(evidence.history.scans.len(), 20);
    }
}
