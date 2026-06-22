use anyhow::{Result, bail};
use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

// ─── Typed validation error ───────────────────────────────────────────────────

/// Validation failures for bundle create/update operations.
/// Using a typed error instead of string matching lets handlers return
/// predictable 400 responses regardless of future message changes.
#[derive(Debug)]
pub enum BundleValidationError {
    NameRequired,
    FrameworkRequired,
    PolicyRequired,
}

impl std::fmt::Display for BundleValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NameRequired      => f.write_str("Bundle name is required"),
            Self::FrameworkRequired => f.write_str("Framework is required"),
            Self::PolicyRequired    => f.write_str("At least one policy is required"),
        }
    }
}

impl std::error::Error for BundleValidationError {}

/// Validate fields common to both create and update.
/// Returns an `anyhow::Error` wrapping a [`BundleValidationError`] so callers
/// can downcast to distinguish validation failures from infrastructure errors.
fn validate_bundle_request(
    name: &str,
    framework: &str,
    policy_ids: &[Uuid],
) -> Result<()> {
    if name.is_empty() {
        return Err(BundleValidationError::NameRequired.into());
    }
    if framework.is_empty() {
        return Err(BundleValidationError::FrameworkRequired.into());
    }
    if policy_ids.is_empty() {
        return Err(BundleValidationError::PolicyRequired.into());
    }
    Ok(())
}

use crate::api::models::{
    ComplianceBundleSummary, ComplianceBundleSystemsResponse, ComplianceControlEvidence,
    ComplianceControlStatus, ComplianceEnvironmentRef, ComplianceEvidenceArtifact,
    ComplianceEvidenceItem, ComplianceEvidenceResponse, ComplianceRollupTotals,
    ComplianceSystemRollup, CreateComplianceBundleRequest, UpdateComplianceBundleRequest,
};

#[derive(Debug, FromRow)]
struct BundleRow {
    id: Uuid,
    name: String,
    framework: String,
    version: String,
    description: Option<String>,
    layer: String,
    owner: String,
    last_review: Option<DateTime<Utc>>,
    policy_ids: Vec<Uuid>,
    env_ids: Vec<Uuid>,
    env_names: Vec<String>,
    env_colors: Vec<String>,
    control_count: i64,
    environment_count: i64,
}

#[derive(Debug, FromRow)]
struct SystemRow {
    id: Uuid,
    hostname: String,
    environment: Option<String>,
    health_status: String,
    critical_cve_count: i32,
    high_cve_count: i32,
}

#[derive(Debug, Clone, FromRow)]
struct PolicyRow {
    id: Uuid,
    name: String,
    description: Option<String>,
    policy_type: String,
    config: Value,
    enabled: bool,
}

fn bundle_from_row(row: BundleRow) -> ComplianceBundleSummary {
    let required_envs = row
        .env_ids
        .into_iter()
        .zip(row.env_names)
        .zip(row.env_colors)
        .map(|((id, name), color_hex)| ComplianceEnvironmentRef {
            id,
            name,
            color_hex,
        })
        .collect();

    ComplianceBundleSummary {
        id: row.id,
        name: row.name,
        framework: row.framework,
        version: row.version,
        description: row.description,
        layer: row.layer,
        owner: row.owner,
        last_review: row.last_review,
        policy_ids: row.policy_ids,
        required_envs,
        control_count: row.control_count,
        environment_count: row.environment_count,
    }
}

pub async fn list_bundles(pool: &PgPool) -> Result<Vec<ComplianceBundleSummary>> {
    let rows = sqlx::query_as::<_, BundleRow>(
        r#"
        SELECT
            b.id,
            b.name,
            b.framework,
            b.version,
            b.description,
            b.layer,
            b.owner,
            b.last_review,
            COALESCE(p.policy_ids, ARRAY[]::uuid[]) AS policy_ids,
            COALESCE(e.env_ids, ARRAY[]::uuid[]) AS env_ids,
            COALESCE(e.env_names, ARRAY[]::text[]) AS env_names,
            COALESCE(e.env_colors, ARRAY[]::text[]) AS env_colors,
            COALESCE(p.control_count, 0)::bigint AS control_count,
            COALESCE(e.environment_count, 0)::bigint AS environment_count
        FROM compliance_bundles b
        LEFT JOIN LATERAL (
            SELECT
                array_agg(cbp.policy_id ORDER BY dp.name) AS policy_ids,
                count(*)::bigint AS control_count
            FROM compliance_bundle_policies cbp
            JOIN deployment_policies dp ON dp.id = cbp.policy_id
            WHERE cbp.bundle_id = b.id
        ) p ON TRUE
        LEFT JOIN LATERAL (
            SELECT
                array_agg(e.id ORDER BY e.name) AS env_ids,
                array_agg(e.name ORDER BY e.name) AS env_names,
                array_agg(COALESCE(e.color_hex, '#6B7280') ORDER BY e.name) AS env_colors,
                count(*)::bigint AS environment_count
            FROM compliance_bundle_environments cbe
            JOIN environments e ON e.id = cbe.environment_id
            WHERE cbe.bundle_id = b.id
        ) e ON TRUE
        ORDER BY b.name ASC
        "#,
    )
    .fetch_all(pool)
    .await?;

    Ok(rows.into_iter().map(bundle_from_row).collect())
}

pub async fn create_bundle(
    pool: &PgPool,
    request: CreateComplianceBundleRequest,
) -> Result<ComplianceBundleSummary> {
    let name = request.name.trim();
    let framework = request.framework.trim();
    let version = request.version.as_deref().unwrap_or("").trim();
    let layer = request.layer.as_deref().unwrap_or("fleet").trim();

    validate_bundle_request(name, framework, &request.policy_ids)?;

    let mut tx = pool.begin().await?;
    let bundle_id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO compliance_bundles (name, framework, version, description, layer)
        VALUES ($1, $2, $3, $4, $5)
        RETURNING id
        "#,
    )
    .bind(name)
    .bind(framework)
    .bind(version)
    .bind(
        request
            .description
            .as_ref()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty()),
    )
    .bind(if layer.is_empty() { "fleet" } else { layer })
    .fetch_one(&mut *tx)
    .await?;

    for policy_id in request.policy_ids {
        sqlx::query(
            r#"
            INSERT INTO compliance_bundle_policies (bundle_id, policy_id)
            VALUES ($1, $2)
            ON CONFLICT DO NOTHING
            "#,
        )
        .bind(bundle_id)
        .bind(policy_id)
        .execute(&mut *tx)
        .await?;
    }

    for environment_id in request.required_envs {
        sqlx::query(
            r#"
            INSERT INTO compliance_bundle_environments (bundle_id, environment_id)
            VALUES ($1, $2)
            ON CONFLICT DO NOTHING
            "#,
        )
        .bind(bundle_id)
        .bind(environment_id)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;

    find_bundle(pool, bundle_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Created bundle was not found"))
}

pub async fn find_bundle(
    pool: &PgPool,
    bundle_id: Uuid,
) -> Result<Option<ComplianceBundleSummary>> {
    let row = sqlx::query_as::<_, BundleRow>(
        r#"
        SELECT
            b.id,
            b.name,
            b.framework,
            b.version,
            b.description,
            b.layer,
            b.owner,
            b.last_review,
            COALESCE(p.policy_ids, ARRAY[]::uuid[]) AS policy_ids,
            COALESCE(e.env_ids, ARRAY[]::uuid[]) AS env_ids,
            COALESCE(e.env_names, ARRAY[]::text[]) AS env_names,
            COALESCE(e.env_colors, ARRAY[]::text[]) AS env_colors,
            COALESCE(p.control_count, 0)::bigint AS control_count,
            COALESCE(e.environment_count, 0)::bigint AS environment_count
        FROM compliance_bundles b
        LEFT JOIN LATERAL (
            SELECT array_agg(policy_id ORDER BY policy_id) AS policy_ids, count(*)::bigint AS control_count
            FROM compliance_bundle_policies
            WHERE bundle_id = b.id
        ) p ON TRUE
        LEFT JOIN LATERAL (
            SELECT
                array_agg(e.id ORDER BY e.name) AS env_ids,
                array_agg(e.name ORDER BY e.name) AS env_names,
                array_agg(COALESCE(e.color_hex, '#6B7280') ORDER BY e.name) AS env_colors,
                count(*)::bigint AS environment_count
            FROM compliance_bundle_environments cbe
            JOIN environments e ON e.id = cbe.environment_id
            WHERE cbe.bundle_id = b.id
        ) e ON TRUE
        WHERE b.id = $1
        "#,
    )
    .bind(bundle_id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(bundle_from_row))
}

pub async fn update_bundle(
    pool: &PgPool,
    bundle_id: Uuid,
    request: UpdateComplianceBundleRequest,
) -> Result<Option<ComplianceBundleSummary>> {
    let name = request.name.trim();
    let framework = request.framework.trim();
    let version = request.version.as_deref().unwrap_or("").trim();

    validate_bundle_request(name, framework, &request.policy_ids)?;

    let mut tx = pool.begin().await?;

    let updated = sqlx::query_scalar::<_, i64>(
        r#"
        UPDATE compliance_bundles
        SET name = $1, framework = $2, version = $3, description = $4,
            last_review = now()
        WHERE id = $5
        RETURNING 1
        "#,
    )
    .bind(name)
    .bind(framework)
    .bind(version)
    .bind(
        request
            .description
            .as_ref()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty()),
    )
    .bind(bundle_id)
    .fetch_optional(&mut *tx)
    .await?;

    if updated.is_none() {
        return Ok(None);
    }

    sqlx::query("DELETE FROM compliance_bundle_policies WHERE bundle_id = $1")
        .bind(bundle_id)
        .execute(&mut *tx)
        .await?;

    for policy_id in request.policy_ids {
        sqlx::query(
            r#"
            INSERT INTO compliance_bundle_policies (bundle_id, policy_id)
            VALUES ($1, $2) ON CONFLICT DO NOTHING
            "#,
        )
        .bind(bundle_id)
        .bind(policy_id)
        .execute(&mut *tx)
        .await?;
    }

    sqlx::query("DELETE FROM compliance_bundle_environments WHERE bundle_id = $1")
        .bind(bundle_id)
        .execute(&mut *tx)
        .await?;

    for env_id in request.required_envs {
        sqlx::query(
            r#"
            INSERT INTO compliance_bundle_environments (bundle_id, environment_id)
            VALUES ($1, $2) ON CONFLICT DO NOTHING
            "#,
        )
        .bind(bundle_id)
        .bind(env_id)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    find_bundle(pool, bundle_id).await
}

pub async fn delete_bundle(pool: &PgPool, bundle_id: Uuid) -> Result<bool> {
    let rows = sqlx::query_scalar::<_, i64>(
        "DELETE FROM compliance_bundles WHERE id = $1 RETURNING 1",
    )
    .bind(bundle_id)
    .fetch_optional(pool)
    .await?;
    Ok(rows.is_some())
}

pub async fn list_bundle_systems(
    pool: &PgPool,
    bundle_id: Uuid,
) -> Result<Option<ComplianceBundleSystemsResponse>> {
    let Some(bundle) = find_bundle(pool, bundle_id).await? else {
        return Ok(None);
    };
    let policies = list_bundle_policies(pool, bundle_id).await?;
    let systems = list_applicable_system_rows(pool, bundle_id).await?;

    let rollups: Vec<_> = systems
        .into_iter()
        .map(|system| system_rollup(system, &policies))
        .collect();
    let totals = totals_for_rollups(&rollups);

    Ok(Some(ComplianceBundleSystemsResponse {
        bundle_id: bundle.id,
        systems: rollups,
        totals,
    }))
}

pub async fn get_system_evidence(
    pool: &PgPool,
    bundle_id: Uuid,
    system_id: Uuid,
) -> Result<Option<ComplianceEvidenceResponse>> {
    if find_bundle(pool, bundle_id).await?.is_none() {
        return Ok(None);
    }

    // Use the same environment predicate as list_applicable_system_rows so that
    // requesting evidence for a system outside the bundle's environment scope
    // returns None (→ 404) rather than fabricated out-of-scope compliance data.
    let system = find_applicable_system_row(pool, bundle_id, system_id).await?;

    let Some(system) = system else {
        return Ok(None);
    };

    let policies = list_bundle_policies(pool, bundle_id).await?;
    let controls = policies
        .into_iter()
        .map(|policy| control_evidence(&system, policy))
        .collect();

    Ok(Some(ComplianceEvidenceResponse {
        bundle_id,
        system_id,
        hostname: system.hostname,
        controls,
    }))
}

/// Fetch a single system row only if it is within the bundle's environment scope.
/// Uses the identical predicate as [`list_applicable_system_rows`] so the two
/// functions can never diverge in which systems they consider applicable.
async fn find_applicable_system_row(
    pool: &PgPool,
    bundle_id: Uuid,
    system_id: Uuid,
) -> Result<Option<SystemRow>> {
    Ok(sqlx::query_as::<_, SystemRow>(
        r#"
        SELECT
            v.id,
            v.hostname,
            v.environment,
            v.health_status,
            v.critical_cve_count,
            v.high_cve_count
        FROM view_system_list v
        LEFT JOIN environments e ON e.name = v.environment
        WHERE v.id = $2
          AND (
            NOT EXISTS (
                SELECT 1 FROM compliance_bundle_environments cbe
                WHERE cbe.bundle_id = $1
            )
            OR EXISTS (
                SELECT 1 FROM compliance_bundle_environments cbe
                WHERE cbe.bundle_id = $1 AND cbe.environment_id = e.id
            )
          )
        "#,
    )
    .bind(bundle_id)
    .bind(system_id)
    .fetch_optional(pool)
    .await?)
}

async fn list_bundle_policies(pool: &PgPool, bundle_id: Uuid) -> Result<Vec<PolicyRow>> {
    Ok(sqlx::query_as::<_, PolicyRow>(
        r#"
        SELECT dp.id, dp.name, dp.description, dp.policy_type, dp.config, dp.enabled
        FROM compliance_bundle_policies cbp
        JOIN deployment_policies dp ON dp.id = cbp.policy_id
        WHERE cbp.bundle_id = $1
        ORDER BY dp.name ASC
        "#,
    )
    .bind(bundle_id)
    .fetch_all(pool)
    .await?)
}

async fn list_applicable_system_rows(pool: &PgPool, bundle_id: Uuid) -> Result<Vec<SystemRow>> {
    Ok(sqlx::query_as::<_, SystemRow>(
        r#"
        SELECT
            v.id,
            v.hostname,
            v.environment,
            v.health_status,
            v.critical_cve_count,
            v.high_cve_count
        FROM view_system_list v
        LEFT JOIN environments e ON e.name = v.environment
        WHERE NOT EXISTS (
            SELECT 1 FROM compliance_bundle_environments cbe
            WHERE cbe.bundle_id = $1
        )
        OR EXISTS (
            SELECT 1 FROM compliance_bundle_environments cbe
            WHERE cbe.bundle_id = $1 AND cbe.environment_id = e.id
        )
        ORDER BY v.hostname ASC
        "#,
    )
    .bind(bundle_id)
    .fetch_all(pool)
    .await?)
}

fn system_rollup(system: SystemRow, policies: &[PolicyRow]) -> ComplianceSystemRollup {
    let mut pass  = 0i64;
    let mut warn  = 0i64;
    let mut fail  = 0i64;
    let waiver    = 0i64;
    // Only policies that were actually evaluated count toward total and score.
    // Disabled and unsupported policies are surfaced as warn but excluded from
    // the denominator so they don't silently deflate the score.
    let mut evaluated_total = 0i64;

    for policy in policies {
        match evaluate_policy(&system, policy) {
            PolicyEval::Evaluated(ComplianceControlStatus::Pass) => {
                pass += 1;
                evaluated_total += 1;
            }
            PolicyEval::Evaluated(ComplianceControlStatus::Warn) => {
                warn += 1;
                evaluated_total += 1;
            }
            PolicyEval::Evaluated(ComplianceControlStatus::Fail) => {
                fail += 1;
                evaluated_total += 1;
            }
            PolicyEval::Evaluated(ComplianceControlStatus::Waiver) => {
                evaluated_total += 1;
            }
            // Disabled or unsupported: count in warn for visibility, but not
            // in evaluated_total so they don't skew the percentage score.
            PolicyEval::Disabled | PolicyEval::Unsupported => {
                warn += 1;
            }
        }
    }

    // total exposed to clients is the full policy count (including disabled/
    // unsupported) so the UI can show "N of M controls evaluated".
    let total = policies.len() as i64;
    let score = if evaluated_total == 0 { 0 } else { (pass * 100) / evaluated_total };

    ComplianceSystemRollup {
        system_id: system.id,
        hostname: system.hostname,
        environment: system.environment,
        applies: true,
        total,
        pass,
        warn,
        fail,
        waiver,
        score,
    }
}

fn totals_for_rollups(rollups: &[ComplianceSystemRollup]) -> ComplianceRollupTotals {
    let mut totals = ComplianceRollupTotals {
        system_count: rollups.len() as i64,
        ..ComplianceRollupTotals::default()
    };

    for rollup in rollups {
        if rollup.fail == 0 && rollup.warn == 0 && rollup.total > 0 {
            totals.fully_compliant_count += 1;
        }
        totals.pass += rollup.pass;
        totals.warn += rollup.warn;
        totals.fail += rollup.fail;
        totals.waiver += rollup.waiver;
        totals.total_controls += rollup.total;
    }

    totals.overall_score = if totals.total_controls == 0 {
        0
    } else {
        (totals.pass * 100) / totals.total_controls
    };
    totals
}

/// Three-way status result so callers can distinguish evaluatable outcomes
/// from controls that were not evaluated (disabled or unsupported type).
enum PolicyEval {
    /// Control was evaluated and produced a compliance outcome.
    Evaluated(ComplianceControlStatus),
    /// Control is disabled — excluded from scores, surfaced as not-evaluated.
    Disabled,
    /// Policy type is not yet supported — not a pass, but not a scored failure.
    Unsupported,
}

fn evaluate_policy(system: &SystemRow, policy: &PolicyRow) -> PolicyEval {
    if !policy.enabled {
        return PolicyEval::Disabled;
    }

    match policy.policy_type.as_str() {
        "require_cf_agent" => {
            // Fail-closed: only known-healthy statuses pass.
            // Unknown / stale / error values → Warn (indeterminate, not a scored pass).
            let status = match system.health_status.as_str() {
                "healthy" | "online" => ComplianceControlStatus::Pass,
                "offline"            => ComplianceControlStatus::Fail,
                _                    => ComplianceControlStatus::Warn,
            };
            PolicyEval::Evaluated(status)
        }
        "require_cve_check" => {
            let max_critical = policy
                .config
                .get("max_critical")
                .and_then(Value::as_i64)
                .unwrap_or(i64::MAX);
            let require_high_justification = policy
                .config
                .get("require_high_justification")
                .and_then(Value::as_bool)
                .unwrap_or(false);

            let status = if i64::from(system.critical_cve_count) > max_critical {
                ComplianceControlStatus::Fail
            } else if require_high_justification && system.high_cve_count > 0 {
                ComplianceControlStatus::Warn
            } else {
                ComplianceControlStatus::Pass
            };
            PolicyEval::Evaluated(status)
        }
        // Unknown policy type: not a fabricated pass, but also not a scored
        // failure — return Warn so the reviewer knows it needs attention.
        _ => PolicyEval::Unsupported,
    }
}

/// Translate a PolicyEval into the ComplianceControlStatus used by rollups and
/// evidence. Disabled and unsupported controls both surface as Warn so they are
/// visible to reviewers, but the rollup excludes them from the total count so
/// they don't deflate scores for controls Crystal Forge can't evaluate.
fn policy_status(system: &SystemRow, policy: &PolicyRow) -> ComplianceControlStatus {
    match evaluate_policy(system, policy) {
        PolicyEval::Evaluated(s) => s,
        PolicyEval::Disabled     => ComplianceControlStatus::Warn,
        PolicyEval::Unsupported  => ComplianceControlStatus::Warn,
    }
}

fn control_evidence(system: &SystemRow, policy: PolicyRow) -> ComplianceControlEvidence {
    let eval   = evaluate_policy(system, &policy);
    let status = match &eval {
        PolicyEval::Evaluated(s)              => s.clone(),
        PolicyEval::Disabled | PolicyEval::Unsupported => ComplianceControlStatus::Warn,
    };

    let severity = match status {
        ComplianceControlStatus::Fail => "high",
        ComplianceControlStatus::Warn => "medium",
        ComplianceControlStatus::Pass | ComplianceControlStatus::Waiver => "low",
    }
    .to_string();

    let summary = match &eval {
        PolicyEval::Evaluated(ComplianceControlStatus::Pass) => format!(
            "{} satisfies {} based on current Crystal Forge evaluation data.",
            system.hostname, policy.name
        ),
        PolicyEval::Evaluated(ComplianceControlStatus::Warn) => format!(
            "{} requires reviewer attention for {}; the evaluation produced an indeterminate result.",
            system.hostname, policy.name
        ),
        PolicyEval::Evaluated(ComplianceControlStatus::Fail) => format!(
            "{} does not satisfy {} based on current Crystal Forge evaluation data.",
            system.hostname, policy.name
        ),
        PolicyEval::Evaluated(ComplianceControlStatus::Waiver) => format!(
            "{} has a waiver recorded for {}.",
            system.hostname, policy.name
        ),
        PolicyEval::Disabled => format!(
            "Policy '{}' is disabled and was not evaluated on {}.",
            policy.name, system.hostname
        ),
        PolicyEval::Unsupported => format!(
            "Policy type '{}' is not yet supported by the Crystal Forge evaluator; '{}' was not evaluated on {}.",
            policy.policy_type, policy.name, system.hostname
        ),
    };

    // The evidence body contains the raw inputs Crystal Forge used.
    // This is evaluation input data, not auditor-collected evidence.
    let body = format!(
        "policy_type={} enabled={} health_status={} critical_cves={} high_cves={}",
        policy.policy_type,
        policy.enabled,
        system.health_status,
        system.critical_cve_count,
        system.high_cve_count
    );

    // Framework mapping: only emit when a real framework control identifier
    // exists on the policy. The policy_type → name string is an internal label,
    // not a framework mapping, so we omit it rather than fabricate one.
    let framework_mapping = String::new();

    ComplianceControlEvidence {
        policy_id: policy.id,
        policy_name: policy.name.clone(),
        status,
        severity,
        summary,
        evidence_items: vec![ComplianceEvidenceItem {
            kind: "policy_eval".to_string(),
            label: policy
                .description
                .clone()
                .unwrap_or_else(|| policy.name.clone()),
            body: body.clone(),
            artifact: Some(ComplianceEvidenceArtifact {
                artifact_type: if policy.policy_type == "require_cve_check" {
                    "cve_scan".to_string()
                } else {
                    "policy_eval".to_string()
                },
                // Accurate label: these are Crystal Forge evaluation inputs,
                // not authoritative auditor-collected evidence artifacts.
                title: "Crystal Forge evaluation inputs".to_string(),
                body,
            }),
        }],
        framework_mapping,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn named_policy(policy_type: &str, name: &str, config: Value, enabled: bool) -> PolicyRow {
        PolicyRow {
            id: Uuid::nil(),
            name: name.to_string(),
            description: None,
            policy_type: policy_type.to_string(),
            config,
            enabled,
        }
    }

    fn policy(policy_type: &str, config: Value, enabled: bool) -> PolicyRow {
        named_policy(policy_type, policy_type, config, enabled)
    }

    fn system(health_status: &str, critical: i32, high: i32) -> SystemRow {
        SystemRow {
            id: Uuid::nil(),
            hostname: "host-01".to_string(),
            environment: Some("prod".to_string()),
            health_status: health_status.to_string(),
            critical_cve_count: critical,
            high_cve_count: high,
        }
    }

    // ── require_cve_check ─────────────────────────────────────────────────────

    #[test]
    fn cve_policy_fails_when_critical_exceeds_threshold() {
        let status = policy_status(
            &system("healthy", 1, 0),
            &policy("require_cve_check", serde_json::json!({ "max_critical": 0 }), true),
        );
        assert!(matches!(status, ComplianceControlStatus::Fail));
    }

    #[test]
    fn cve_policy_passes_when_within_threshold() {
        let status = policy_status(
            &system("healthy", 0, 0),
            &policy("require_cve_check", serde_json::json!({ "max_critical": 0 }), true),
        );
        assert!(matches!(status, ComplianceControlStatus::Pass));
    }

    #[test]
    fn cve_policy_warns_when_high_justification_required_and_high_cves_present() {
        let status = policy_status(
            &system("healthy", 0, 3),
            &policy(
                "require_cve_check",
                serde_json::json!({ "max_critical": 0, "require_high_justification": true }),
                true,
            ),
        );
        assert!(matches!(status, ComplianceControlStatus::Warn));
    }

    // ── require_cf_agent ──────────────────────────────────────────────────────

    #[test]
    fn agent_policy_passes_for_healthy_status() {
        let status = policy_status(
            &system("healthy", 0, 0),
            &policy("require_cf_agent", serde_json::json!({}), true),
        );
        assert!(matches!(status, ComplianceControlStatus::Pass));
    }

    #[test]
    fn agent_policy_passes_for_online_status() {
        let status = policy_status(
            &system("online", 0, 0),
            &policy("require_cf_agent", serde_json::json!({}), true),
        );
        assert!(matches!(status, ComplianceControlStatus::Pass));
    }

    #[test]
    fn agent_policy_fails_for_offline_status() {
        let status = policy_status(
            &system("offline", 0, 0),
            &policy("require_cf_agent", serde_json::json!({}), true),
        );
        assert!(matches!(status, ComplianceControlStatus::Fail));
    }

    #[test]
    fn agent_policy_warns_for_unknown_status_not_fabricate_pass() {
        // Any unrecognised status must not pass — it is indeterminate.
        for unknown in &["stale", "error", "unhealthy", "degraded", "unknown", ""] {
            let status = policy_status(
                &system(unknown, 0, 0),
                &policy("require_cf_agent", serde_json::json!({}), true),
            );
            assert!(
                matches!(status, ComplianceControlStatus::Warn),
                "expected Warn for health_status={unknown:?}, got {status:?}"
            );
        }
    }

    // ── disabled policies ─────────────────────────────────────────────────────

    #[test]
    fn disabled_policy_surfaces_as_warn_not_pass() {
        let status = policy_status(
            &system("healthy", 0, 0),
            &policy("require_cf_agent", serde_json::json!({}), false),
        );
        assert!(matches!(status, ComplianceControlStatus::Warn));
    }

    #[test]
    fn disabled_policy_is_excluded_from_score_denominator() {
        // A disabled policy contributes warn to the counts but must not count
        // toward evaluated_total, so a host with only disabled policies scores 0
        // rather than 100%.
        let sys = system("healthy", 0, 0);
        let rollup = system_rollup(
            sys,
            &[policy("require_cf_agent", serde_json::json!({}), false)],
        );
        // warn count is 1 (disabled shows as warn for visibility)
        assert_eq!(rollup.warn, 1, "disabled policy should surface as warn");
        // score must be 0 — no evaluated policies means the score is undefined/0
        assert_eq!(rollup.score, 0, "score with only disabled policies should be 0");
        // total is 1 (the policy exists in the bundle)
        assert_eq!(rollup.total, 1);
    }

    // ── unsupported policy types ──────────────────────────────────────────────

    #[test]
    fn unknown_policy_warns_not_fabricating_pass() {
        let status = policy_status(
            &system("healthy", 0, 0),
            &policy("custom_check", serde_json::json!({}), true),
        );
        assert!(matches!(status, ComplianceControlStatus::Warn));
    }

    #[test]
    fn unsupported_policy_excluded_from_score_denominator() {
        let sys = system("healthy", 0, 0);
        let rollup = system_rollup(
            sys,
            &[policy("custom_unsupported_type", serde_json::json!({}), true)],
        );
        assert_eq!(rollup.warn, 1, "unsupported policy should surface as warn");
        assert_eq!(rollup.score, 0, "score with only unsupported policies should be 0");
    }

    // ── evidence labels ───────────────────────────────────────────────────────

    #[test]
    fn evidence_artifact_title_is_not_authoritative() {
        let sys = system("healthy", 0, 0);
        let pol = policy("require_cf_agent", serde_json::json!({}), true);
        let ev  = control_evidence(&sys, pol);
        let artifact = ev.evidence_items[0].artifact.as_ref().unwrap();
        assert_ne!(
            artifact.title, "Authoritative Crystal Forge signal",
            "artifact title must not claim to be authoritative auditor evidence"
        );
        assert!(
            artifact.title.contains("evaluation inputs")
                || artifact.title.contains("Crystal Forge"),
            "artifact title should describe evaluation inputs, got: {:?}",
            artifact.title
        );
    }

    #[test]
    fn framework_mapping_is_empty_when_no_real_mapping_exists() {
        let sys = system("healthy", 0, 0);
        let pol = named_policy("require_cf_agent", "CF Agent Check", serde_json::json!({}), true);
        let ev  = control_evidence(&sys, pol);
        assert!(
            ev.framework_mapping.is_empty(),
            "framework_mapping must be empty when no real framework control ID exists, \
             got: {:?}",
            ev.framework_mapping
        );
    }
}
