//! Evaluates and persists multi-phase composite policy enforcement.
//!
//! Evaluation, scan, and deployment outcomes are tied to an exact system
//! target and effective policy set. Authorization updates assessment aggregates
//! and any guarded target state in one transaction so callers cannot consume
//! stale passing evidence.

use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::compliance::resolver::{
    AssignmentMode, EffectivePolicySet, ResolutionOutcome, resolve_system_effective_policies_in_tx,
};
use crate::models::deployment_policies::{
    CompositePolicyConfig, CompositeRuleKind, CompositeRuleOutcome, CveBlockSeverity,
    EnforcementOutcome, EnforcementPhase, PoliciesByConfiguration, PolicyCheckResult,
    TimeWindowConfig, composite_config_digest, deserialize_policy_type_config,
};
use crate::queries::system_events::set_pending_deployment_target_tx;
use crate::services::time_window_policy;

// CONCURRENCY: This advisory-lock namespace spells "POAM" in ASCII. Keep it
// stable so derivation evidence and deployment publication share one key space.
const POAM_DERIVATION_LOCK_NAMESPACE: i32 = 0x504F_414D;

/// Describes the aggregate composite-policy decision for one system target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompositeAuthorization {
    /// Contains the aggregate outcome across all applicable assessments.
    pub outcome: EnforcementOutcome,
    /// Identifies the exact assessment rows used for the decision.
    pub assessments: Vec<Uuid>,
    /// Explains the aggregate decision for diagnostics.
    pub detail: String,
}

impl CompositeAuthorization {
    /// Returns whether every applicable composite assessment passed.
    pub fn allowed(&self) -> bool {
        self.outcome == EnforcementOutcome::Pass
    }
}

/// Describes an atomic desired-target delivery claim and its authorization.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetDeliveryAuthorization {
    /// Contains the claimed target, or `None` when no target was delivered.
    pub target: Option<String>,
    /// Contains the composite-policy decision made while claiming the target.
    pub authorization: CompositeAuthorization,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct LatestScan {
    id: Uuid,
    derivation_id: i32,
    status: String,
    critical_count: i32,
    high_count: i32,
    medium_count: i32,
    low_count: i32,
    scan_metadata: Option<serde_json::Value>,
    created_at: Option<DateTime<Utc>>,
    completed_at: Option<DateTime<Utc>>,
    composite_phase_order: i64,
}

#[derive(Debug)]
struct PolicyContext {
    lineage_id: Uuid,
    version_id: Uuid,
    config: CompositePolicyConfig,
    config_json: serde_json::Value,
    config_digest: String,
}

#[derive(Debug, sqlx::FromRow)]
struct PersistedRule {
    assessment_id: Uuid,
    rule_id: Uuid,
    source_scan_id: Option<Uuid>,
    source_scan_order: Option<i64>,
}

enum AuthorizationAction<'a> {
    Check {
        expected_derivation_id: Option<i32>,
    },
    SetDesired {
        source: &'a str,
        evaluation_snapshot_id: Option<Uuid>,
        expected_derivation_id: Option<i32>,
    },
    ClaimDelivery {
        expected_target: &'a str,
    },
}

fn outcome_str(outcome: EnforcementOutcome) -> &'static str {
    match outcome {
        EnforcementOutcome::Pass => "pass",
        EnforcementOutcome::Fail => "fail",
        EnforcementOutcome::Error => "error",
        EnforcementOutcome::NotChecked => "not_checked",
    }
}

fn parse_outcome(outcome: &str) -> EnforcementOutcome {
    match outcome {
        "pass" => EnforcementOutcome::Pass,
        "fail" => EnforcementOutcome::Fail,
        "error" => EnforcementOutcome::Error,
        _ => EnforcementOutcome::NotChecked,
    }
}

fn phase_str(phase: EnforcementPhase) -> &'static str {
    match phase {
        EnforcementPhase::Evaluation => "evaluation",
        EnforcementPhase::Scan => "scan",
        EnforcementPhase::Deployment => "deployment",
    }
}

/// Seeds NotChecked rows for the exact active evaluation attempt.
///
/// Starting a newer attempt supersedes prior evidence; merely queueing a retry
/// does not, so the terminal outcome remains readable until that retry starts.
///
/// # Errors
///
/// Returns an error when the active attempt cannot be loaded or when its
/// evidence and affected assessment aggregates cannot be updated atomically.
pub async fn initialize_eval_passed_attempt(
    pool: &PgPool,
    commit_id: i32,
    attempt_number: i32,
    policies: &PoliciesByConfiguration,
) -> Result<()> {
    let mut configurations = Vec::new();
    let mut version_ids = Vec::new();
    let mut rule_ids = Vec::new();
    for (configuration, assigned) in policies {
        for policy in assigned {
            let crate::models::deployment_policies::DeploymentPolicy::Composite { config } =
                &policy.policy
            else {
                continue;
            };
            for rule in &config.rules {
                if matches!(rule.rule, CompositeRuleKind::EvalPassed(_)) {
                    configurations.push(configuration.clone());
                    version_ids.push(policy.policy_id);
                    rule_ids.push(rule.id);
                }
            }
        }
    }
    if configurations.is_empty() {
        return Ok(());
    }

    let mut tx = pool.begin().await?;
    let attempt_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM evaluation_attempts WHERE commit_id = $1 AND attempt_number = $2 AND status = 'in_progress' FOR SHARE",
    )
    .bind(commit_id)
    .bind(attempt_number)
    .fetch_one(&mut *tx)
    .await?;
    sqlx::query(
        r#"
        UPDATE composite_eval_attempt_rule_results result
        SET superseded_at = COALESCE(result.superseded_at, NOW())
        FROM evaluation_attempts attempt
        WHERE result.evaluation_attempt_id = attempt.id
          AND attempt.commit_id = $1
          AND result.evaluation_attempt_id <> $2
          AND result.superseded_at IS NULL
        "#,
    )
    .bind(commit_id)
    .bind(attempt_id)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        r#"
        INSERT INTO composite_eval_attempt_rule_results (
            evaluation_attempt_id, system_id, configuration_name,
            policy_version_id, rule_id, outcome, detail, evidence
        )
        SELECT $1,
               CASE WHEN COUNT(system.id) = 1
                    THEN (ARRAY_AGG(system.id ORDER BY system.id))[1]
                    ELSE NULL END,
               input.configuration_name, input.policy_version_id, input.rule_id,
               'not_checked', 'Target evaluation is pending',
               jsonb_build_object('evaluation_attempt_id', $1, 'attempt_number', $5,
                                  'configuration', input.configuration_name)
        FROM UNNEST($2::text[], $3::uuid[], $4::uuid[])
             AS input(configuration_name, policy_version_id, rule_id)
        JOIN evaluation_attempts attempt ON attempt.id = $1
        JOIN commits commit ON commit.id = attempt.commit_id
        LEFT JOIN systems system
          ON system.flake_id = commit.flake_id
         AND COALESCE(NULLIF(BTRIM(system.system_configuration_name), ''), system.hostname)
             = input.configuration_name
        GROUP BY input.configuration_name, input.policy_version_id, input.rule_id
        ON CONFLICT (evaluation_attempt_id, configuration_name, policy_version_id, rule_id)
        DO UPDATE SET outcome = 'not_checked', detail = EXCLUDED.detail,
                      evidence = EXCLUDED.evidence, evaluated_at = NOW(), superseded_at = NULL
        "#,
    )
    .bind(attempt_id)
    .bind(&configurations)
    .bind(&version_ids)
    .bind(&rule_ids)
    .bind(attempt_number)
    .execute(&mut *tx)
    .await?;
    reset_eval_passed_assessments_for_attempt_in_tx(&mut tx, attempt_id, attempt_number).await?;
    tx.commit().await?;
    Ok(())
}

async fn lock_eval_passed_assessments_for_attempt_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    attempt_id: Uuid,
) -> Result<()> {
    let system_ids: Vec<Uuid> = sqlx::query_scalar(
        r#"
        SELECT DISTINCT assessment.system_id
        FROM composite_policy_assessments assessment
        JOIN derivations derivation ON derivation.id = assessment.derivation_id
        JOIN evaluation_attempts attempt ON attempt.commit_id = derivation.commit_id
        WHERE attempt.id = $1
          AND EXISTS (
              SELECT 1
              FROM composite_eval_attempt_rule_results attempt_result
              WHERE attempt_result.evaluation_attempt_id = attempt.id
                AND attempt_result.policy_version_id = assessment.policy_version_id
                AND EXISTS (
                    SELECT 1 FROM composite_policy_rule_results rule_result
                    WHERE rule_result.assessment_id = assessment.id
                      AND rule_result.rule_id = attempt_result.rule_id
                      AND rule_result.kind = 'eval_passed'
                )
          )
        ORDER BY assessment.system_id
        "#,
    )
    .bind(attempt_id)
    .fetch_all(&mut **tx)
    .await?;
    for system_id in system_ids {
        // CONCURRENCY: Deployment authorization and POA&M closure acquire the
        // same system/finding keys before assessment rows. Keep that order so
        // neither can observe a rule transition without its new aggregate.
        lock_poam_findings_for_system_tx(tx, system_id).await?;
    }
    Ok(())
}

/// Resets exact-commit `eval_passed` assessments when a new attempt starts.
///
/// The transition does not wait for policy loading or evaluator setup. This
/// prevents deployment authorization from consuming an earlier Pass during
/// the interval between claiming the retry and initializing attempt evidence.
///
/// # Errors
///
/// Returns an error when PostgreSQL cannot lock or update the assessments.
pub(crate) async fn reset_eval_passed_assessments_for_started_attempt_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    commit_id: i32,
    attempt_id: Uuid,
    attempt_number: i32,
) -> Result<()> {
    let system_ids: Vec<Uuid> = sqlx::query_scalar(
        r#"
        SELECT DISTINCT assessment.system_id
        FROM composite_policy_assessments assessment
        JOIN derivations derivation ON derivation.id = assessment.derivation_id
        JOIN composite_policy_rule_results rule_result
          ON rule_result.assessment_id = assessment.id
        WHERE derivation.commit_id = $1 AND rule_result.kind = 'eval_passed'
        ORDER BY assessment.system_id
        "#,
    )
    .bind(commit_id)
    .fetch_all(&mut **tx)
    .await?;
    for system_id in system_ids {
        // CONCURRENCY: Use the same system/finding lock order as deployment
        // authorization and POA&M closure before changing assessment rows.
        lock_poam_findings_for_system_tx(tx, system_id).await?;
    }
    let assessment_ids: Vec<Uuid> = sqlx::query_scalar(
        r#"
        UPDATE composite_policy_rule_results rule_result
        SET outcome = 'not_checked', blocking = TRUE,
            detail = 'Target evaluation retry is in progress',
            evidence = jsonb_build_object(
                'evaluation_attempt_id', $2,
                'attempt_number', $3,
                'terminal_outcome', 'pending'
            ),
            evaluated_at = NOW()
        FROM composite_policy_assessments assessment
        JOIN derivations derivation ON derivation.id = assessment.derivation_id
        WHERE rule_result.assessment_id = assessment.id
          AND derivation.commit_id = $1
          AND rule_result.kind = 'eval_passed'
        RETURNING assessment.id
        "#,
    )
    .bind(commit_id)
    .bind(attempt_id)
    .bind(attempt_number)
    .fetch_all(&mut **tx)
    .await?;
    // INVARIANT: Claiming a newer attempt and revoking prior Pass assessments
    // are one transaction. Authorization sees either state, never a stale Pass
    // paired with an in-progress retry.
    recompute_aggregates(tx, &assessment_ids).await?;
    Ok(())
}

async fn reset_eval_passed_assessments_for_attempt_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    attempt_id: Uuid,
    attempt_number: i32,
) -> Result<()> {
    lock_eval_passed_assessments_for_attempt_in_tx(tx, attempt_id).await?;
    let assessment_ids: Vec<Uuid> = sqlx::query_scalar(
        r#"
        UPDATE composite_policy_rule_results rule_result
        SET outcome = 'not_checked', blocking = TRUE,
            detail = 'Target evaluation retry is in progress',
            evidence = jsonb_build_object(
                'evaluation_attempt_id', $1,
                'attempt_number', $2,
                'terminal_outcome', 'pending'
            ),
            evaluated_at = NOW()
        FROM composite_policy_assessments assessment
        JOIN derivations derivation ON derivation.id = assessment.derivation_id
        JOIN evaluation_attempts attempt ON attempt.commit_id = derivation.commit_id
        WHERE rule_result.assessment_id = assessment.id
          AND attempt.id = $1
          AND rule_result.kind = 'eval_passed'
          AND EXISTS (
              SELECT 1
              FROM composite_eval_attempt_rule_results attempt_result
              WHERE attempt_result.evaluation_attempt_id = attempt.id
                AND attempt_result.policy_version_id = assessment.policy_version_id
                AND attempt_result.rule_id = rule_result.rule_id
          )
        RETURNING assessment.id
        "#,
    )
    .bind(attempt_id)
    .bind(attempt_number)
    .fetch_all(&mut **tx)
    .await?;
    // INVARIANT: Starting an attempt invalidates every prior eval_passed Pass
    // for this commit before the transaction becomes visible. Authorization
    // can observe only the old terminal state or this NotChecked aggregate.
    recompute_aggregates(tx, &assessment_ids).await?;
    Ok(())
}

/// Fails provisional `eval_passed` evidence for an infrastructure failure.
///
/// Existing Fail or Error outcomes remain authoritative. Pass and NotChecked
/// outcomes become Error in both attempt evidence and exact-target composite
/// assessments before the evaluation failure transaction commits.
///
/// # Errors
///
/// Returns an error when PostgreSQL cannot lock or update the evidence.
pub(crate) async fn fail_eval_passed_attempt_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    attempt_id: Uuid,
    detail: &str,
    failure_class: &str,
) -> Result<()> {
    lock_eval_passed_assessments_for_attempt_in_tx(tx, attempt_id).await?;
    sqlx::query(
        r#"
        UPDATE composite_eval_attempt_rule_results
        SET outcome = 'error', detail = $2,
            evidence = evidence || jsonb_build_object(
                'terminal_outcome', 'error', 'failure_class', $3
            ),
            evaluated_at = NOW()
        WHERE evaluation_attempt_id = $1
          AND outcome IN ('pass', 'not_checked')
          AND superseded_at IS NULL
        "#,
    )
    .bind(attempt_id)
    .bind(detail)
    .bind(failure_class)
    .execute(&mut **tx)
    .await?;

    let assessment_ids: Vec<Uuid> = sqlx::query_scalar(
        r#"
        UPDATE composite_policy_rule_results rule_result
        SET outcome = 'error', blocking = TRUE, detail = $2,
            evidence = rule_result.evidence || jsonb_build_object(
                'evaluation_attempt_id', $1,
                'terminal_outcome', 'error',
                'failure_class', $3
            ),
            evaluated_at = NOW()
        FROM composite_policy_assessments assessment
        JOIN derivations derivation ON derivation.id = assessment.derivation_id
        JOIN evaluation_attempts attempt ON attempt.commit_id = derivation.commit_id
        WHERE rule_result.assessment_id = assessment.id
          AND attempt.id = $1
          AND rule_result.kind = 'eval_passed'
          AND rule_result.outcome IN ('pass', 'not_checked')
          AND EXISTS (
              SELECT 1
              FROM composite_eval_attempt_rule_results attempt_result
              WHERE attempt_result.evaluation_attempt_id = attempt.id
                AND attempt_result.policy_version_id = assessment.policy_version_id
                AND attempt_result.rule_id = rule_result.rule_id
          )
        RETURNING assessment.id
        "#,
    )
    .bind(attempt_id)
    .bind(detail)
    .bind(failure_class)
    .fetch_all(&mut **tx)
    .await?;
    // INVARIANT: A later infrastructure failure revokes a per-system Pass.
    // Recompute in this transaction so deployment cannot consume stale Pass.
    recompute_aggregates(tx, &assessment_ids).await?;
    Ok(())
}

/// Persists authoritative `eval_passed` outcomes for one evaluated system.
///
/// The supplied policy check is the evaluator result for this configuration.
/// Terminal metadata errors therefore persist as Error and never pass
/// provisionally while commit finalization is pending.
///
/// # Errors
///
/// Returns an error when PostgreSQL cannot persist an outcome.
pub async fn persist_eval_passed_for_system_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    commit_id: i32,
    attempt_number: i32,
    system_id: Uuid,
    policy_check: &PolicyCheckResult,
) -> Result<()> {
    for (policy_version_id, result) in &policy_check.assigned_results {
        for outcome in &result.composite_outcomes {
            if outcome.kind != "eval_passed" {
                continue;
            }
            sqlx::query(
                r#"
                INSERT INTO composite_eval_attempt_rule_results (
                    evaluation_attempt_id, system_id, configuration_name,
                    policy_version_id, rule_id, outcome, detail, evidence
                )
                SELECT attempt.id, $3, $4, $5, $6, $7, $8,
                       $9
                FROM evaluation_attempts attempt
                WHERE attempt.commit_id = $1 AND attempt.attempt_number = $2
                  AND attempt.status = 'in_progress'
                ON CONFLICT (evaluation_attempt_id, configuration_name, policy_version_id, rule_id)
                DO UPDATE SET system_id = EXCLUDED.system_id, outcome = EXCLUDED.outcome,
                              detail = EXCLUDED.detail, evidence = EXCLUDED.evidence,
                              evaluated_at = NOW(), superseded_at = NULL
                "#,
            )
            .bind(commit_id)
            .bind(attempt_number)
            .bind(system_id)
            .bind(&policy_check.system_name)
            .bind(policy_version_id)
            .bind(outcome.rule_id)
            .bind(outcome_str(outcome.outcome))
            .bind(&outcome.detail)
            .bind(&outcome.evidence)
            .execute(&mut **tx)
            .await?;
        }
    }
    Ok(())
}

/// Replaces provisional `eval_passed` evidence with terminal evaluation outcomes.
///
/// The update covers both attempt-scoped diagnostics and deployed-target
/// assessments consumed by POA&M verification. Assessment aggregates are
/// recomputed before this transaction can commit.
///
/// # Errors
///
/// Returns an error when PostgreSQL cannot resolve, lock, or update the
/// affected evaluation evidence.
pub async fn persist_eval_passed_terminal_checks_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    commit_id: i32,
    attempt_number: i32,
    checks: &[PolicyCheckResult],
) -> Result<()> {
    let mut assessment_ids = Vec::new();
    for check in checks {
        let system_id: Option<Uuid> = sqlx::query_scalar(
            r#"
            SELECT system.id
            FROM systems system
            JOIN commits commit ON commit.id = $1 AND commit.flake_id = system.flake_id
            WHERE COALESCE(NULLIF(BTRIM(system.system_configuration_name), ''), system.hostname) = $2
            "#,
        )
        .bind(commit_id)
        .bind(&check.system_name)
        .fetch_optional(&mut **tx)
        .await?;
        if let Some(system_id) = system_id {
            // CONCURRENCY: Finalization can replace a provisional Pass that is
            // visible to POA&M closure. Use the standard writer lock order so
            // closure cannot consume the assessment between the rule update
            // and aggregate recomputation.
            lock_poam_system_key_tx(tx, system_id).await?;
            sqlx::query(
                r#"SELECT lock_poam_finding_key(system_id,policy_lineage_id)
                   FROM poam_findings WHERE system_id=$1
                   ORDER BY system_id,policy_lineage_id"#,
            )
            .bind(system_id)
            .execute(&mut **tx)
            .await?;
        }
        for (version_id, result) in &check.assigned_results {
            for outcome in result
                .composite_outcomes
                .iter()
                .filter(|outcome| outcome.kind == "eval_passed")
            {
                sqlx::query(
                    r#"
                    UPDATE composite_eval_attempt_rule_results result
                    SET outcome = $4, detail = $5, evidence = $6,
                        evaluated_at = NOW(), superseded_at = NULL
                    FROM evaluation_attempts attempt
                    WHERE result.evaluation_attempt_id = attempt.id
                      AND attempt.commit_id = $1 AND attempt.attempt_number = $2
                      AND result.configuration_name = $3
                      AND result.policy_version_id = $7 AND result.rule_id = $8
                    "#,
                )
                .bind(commit_id)
                .bind(attempt_number)
                .bind(&check.system_name)
                .bind(outcome_str(outcome.outcome))
                .bind(&outcome.detail)
                .bind(&outcome.evidence)
                .bind(version_id)
                .bind(outcome.rule_id)
                .execute(&mut **tx)
                .await?;

                if let Some(system_id) = system_id {
                    let updated: Vec<Uuid> = sqlx::query_scalar(
                        r#"
                        UPDATE composite_policy_rule_results rule_result
                        SET outcome = $3, blocking = $3 <> 'pass', detail = $4,
                            evidence = $5, evaluated_at = NOW()
                        FROM composite_policy_assessments assessment
                        JOIN derivations derivation ON derivation.id = assessment.derivation_id
                        WHERE rule_result.assessment_id = assessment.id
                          AND derivation.commit_id = $1
                          AND assessment.system_id = $2
                          AND assessment.policy_version_id = $6
                          AND rule_result.rule_id = $7
                          AND rule_result.kind = 'eval_passed'
                        RETURNING assessment.id
                        "#,
                    )
                    .bind(commit_id)
                    .bind(system_id)
                    .bind(outcome_str(outcome.outcome))
                    .bind(&outcome.detail)
                    .bind(&outcome.evidence)
                    .bind(version_id)
                    .bind(outcome.rule_id)
                    .fetch_all(&mut **tx)
                    .await?;
                    assessment_ids.extend(updated);
                }
            }
        }
    }
    assessment_ids.sort_unstable();
    assessment_ids.dedup();
    recompute_aggregates(tx, &assessment_ids).await?;
    Ok(())
}

fn expected_phase(rule: &CompositeRuleKind) -> EnforcementPhase {
    match rule {
        CompositeRuleKind::CveBlock(_) => EnforcementPhase::Scan,
        CompositeRuleKind::TimeWindow(_) => EnforcementPhase::Deployment,
        _ => EnforcementPhase::Evaluation,
    }
}

fn policy_contexts(resolved: &EffectivePolicySet) -> Result<Vec<PolicyContext>> {
    resolved
        .policies
        .iter()
        .filter(|policy| {
            policy.policy_type == "composite"
                && matches!(policy.effective_mode, AssignmentMode::Enforce)
        })
        .map(|policy| {
            let config = deserialize_policy_type_config("composite", &policy.effective_config)
                .map_err(anyhow::Error::msg)?
                .context("Composite validator returned no config")?;
            let config_json = serde_json::to_value(&config)?;
            Ok(PolicyContext {
                lineage_id: policy.policy_lineage_id,
                version_id: policy.policy_version_id,
                config_digest: composite_config_digest(&config),
                config,
                config_json,
            })
        })
        .collect()
}

fn evaluation_outcomes(
    policy_version_id: Uuid,
    config: &CompositePolicyConfig,
    policy_results: &serde_json::Value,
) -> Vec<CompositeRuleOutcome> {
    let policy_result = policy_results
        .get("assigned")
        .and_then(|assigned| assigned.get(policy_version_id.to_string()));
    let expected_digest = composite_config_digest(config);
    let digest_matches = policy_result
        .and_then(|policy| policy.get("config_digest"))
        .and_then(|digest| digest.as_str())
        == Some(expected_digest.as_str());
    let persisted = policy_result
        .and_then(|policy| policy.get("rule_outcomes"))
        .and_then(|outcomes| outcomes.as_array());

    config
        .rules
        .iter()
        .filter(|rule| expected_phase(&rule.rule) == EnforcementPhase::Evaluation)
        .map(|rule| {
            if !digest_matches {
                return CompositeRuleOutcome {
                    rule_id: rule.id,
                    kind: rule.rule.kind().to_string(),
                    phase: EnforcementPhase::Evaluation,
                    outcome: EnforcementOutcome::Error,
                    blocking: true,
                    detail: "Evaluator evidence has a stale effective configuration digest"
                        .to_string(),
                    evidence: serde_json::json!({ "policy_version_id": policy_version_id }),
                };
            }
            persisted
                .and_then(|outcomes| {
                    outcomes.iter().find(|outcome| {
                        outcome
                            .get("rule_id")
                            .and_then(|id| id.as_str())
                            .and_then(|id| Uuid::parse_str(id).ok())
                            == Some(rule.id)
                            && outcome.get("kind").and_then(|kind| kind.as_str())
                                == Some(rule.rule.kind())
                    })
                })
                .and_then(|outcome| {
                    serde_json::from_value::<CompositeRuleOutcome>(outcome.clone()).ok()
                })
                .unwrap_or_else(|| CompositeRuleOutcome {
                    rule_id: rule.id,
                    kind: rule.rule.kind().to_string(),
                    phase: EnforcementPhase::Evaluation,
                    outcome: EnforcementOutcome::Error,
                    blocking: true,
                    detail: "Exact evaluator rule evidence is missing or stale".to_string(),
                    evidence: serde_json::json!({ "policy_version_id": policy_version_id }),
                })
        })
        .collect()
}

fn scan_outcome(
    rule_id: Uuid,
    rule: &CompositeRuleKind,
    scan: Option<&LatestScan>,
) -> CompositeRuleOutcome {
    let CompositeRuleKind::CveBlock(config) = rule else {
        unreachable!("scan outcome called for a non-scan rule")
    };
    let (outcome, detail, evidence) = match scan {
        None => (
            EnforcementOutcome::NotChecked,
            "No CVE scan attempt exists for the exact derivation".to_string(),
            serde_json::json!({}),
        ),
        Some(scan) if matches!(scan.status.as_str(), "pending" | "in_progress") => (
            EnforcementOutcome::NotChecked,
            format!("CVE scan {} is {}", scan.id, scan.status),
            serde_json::json!({ "scan_id": scan.id, "status": scan.status, "created_at": scan.created_at }),
        ),
        Some(scan) if scan.status == "failed" => (
            EnforcementOutcome::Error,
            format!("CVE scan {} failed", scan.id),
            serde_json::json!({
                "scan_id": scan.id,
                "status": scan.status,
                "scan_metadata": scan.scan_metadata,
                "completed_at": scan.completed_at,
            }),
        ),
        Some(scan) if scan.status == "completed" => {
            let count = match config.severity {
                CveBlockSeverity::Critical => scan.critical_count,
                CveBlockSeverity::High => scan.high_count,
                CveBlockSeverity::Medium => scan.medium_count,
                CveBlockSeverity::Low => scan.low_count,
            };
            let outcome = if count >= 0 && count as u32 <= config.max_allowed {
                EnforcementOutcome::Pass
            } else {
                EnforcementOutcome::Fail
            };
            (
                outcome,
                format!(
                    "{} {:?} CVEs found; maximum allowed is {}",
                    count, config.severity, config.max_allowed
                ),
                serde_json::json!({
                    "scan_id": scan.id,
                    "status": scan.status,
                    "severity": config.severity,
                    "count": count,
                    "max_allowed": config.max_allowed,
                    "completed_at": scan.completed_at,
                }),
            )
        }
        Some(scan) => (
            EnforcementOutcome::Error,
            format!(
                "CVE scan {} has unsupported status {}",
                scan.id, scan.status
            ),
            serde_json::json!({ "scan_id": scan.id, "status": scan.status }),
        ),
    };
    CompositeRuleOutcome {
        rule_id,
        kind: rule.kind().to_string(),
        phase: EnforcementPhase::Scan,
        outcome,
        blocking: outcome != EnforcementOutcome::Pass,
        detail,
        evidence,
    }
}

fn deployment_outcome(
    rule_id: Uuid,
    rule: &CompositeRuleKind,
    now: DateTime<Utc>,
) -> CompositeRuleOutcome {
    let CompositeRuleKind::TimeWindow(window) = rule else {
        unreachable!("deployment outcome called for a non-deployment rule")
    };
    let decision = time_window_policy::check_time_window_at(
        &TimeWindowConfig {
            description: "Composite deployment window".to_string(),
            days: window.days.clone(),
            start_time: window.from.clone(),
            end_time: window.to.clone(),
            timezone: window.tz.clone(),
            action: "block".to_string(),
        },
        now,
    );
    let outcome = if decision.deployment_allowed {
        EnforcementOutcome::Pass
    } else {
        EnforcementOutcome::Fail
    };
    CompositeRuleOutcome {
        rule_id,
        kind: rule.kind().to_string(),
        phase: EnforcementPhase::Deployment,
        outcome,
        blocking: outcome != EnforcementOutcome::Pass,
        detail: decision
            .reason
            .unwrap_or_else(|| "Deployment is within the configured window".to_string()),
        evidence: serde_json::json!({
            "evaluated_at": now,
            "days": window.days,
            "from": window.from,
            "to": window.to,
            "timezone": window.tz,
        }),
    }
}

async fn upsert_assessments_with_placeholders(
    tx: &mut Transaction<'_, Postgres>,
    system_id: Uuid,
    derivation_id: i32,
    target_store_path: &str,
    effective_set_digest: &str,
    policies: &[PolicyContext],
) -> Result<Vec<(Uuid, Uuid)>> {
    if policies.is_empty() {
        return Ok(Vec::new());
    }
    let target_matches: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM derivations WHERE id = $1 AND expected_store_path = $2)",
    )
    .bind(derivation_id)
    .bind(target_store_path)
    .fetch_one(&mut **tx)
    .await?;
    if !target_matches {
        bail!(
            "Composite assessment target does not match the derivation's current evaluation target"
        );
    }
    sqlx::query(
        r#"
        INSERT INTO composite_policy_derivation_targets (derivation_id, target_store_path)
        VALUES ($1, $2)
        ON CONFLICT (derivation_id, target_store_path) DO NOTHING
        "#,
    )
    .bind(derivation_id)
    .bind(target_store_path)
    .execute(&mut **tx)
    .await?;
    let lineage_ids = policies.iter().map(|p| p.lineage_id).collect::<Vec<_>>();
    let version_ids = policies.iter().map(|p| p.version_id).collect::<Vec<_>>();
    let config_digests = policies
        .iter()
        .map(|p| p.config_digest.clone())
        .collect::<Vec<_>>();
    let configs = policies
        .iter()
        .map(|p| p.config_json.clone())
        .collect::<Vec<_>>();
    let assessments = sqlx::query_as::<_, (Uuid, Uuid)>(
        r#"
        INSERT INTO composite_policy_assessments (
            system_id, derivation_id, target_store_path, policy_lineage_id,
            policy_version_id, effective_set_digest, effective_config_digest,
            effective_config
        )
        SELECT $1, $2, $3, policy_lineage_id, policy_version_id, $4,
               config_digest, config
        FROM UNNEST($5::uuid[], $6::uuid[], $7::text[], $8::jsonb[])
             AS input(policy_lineage_id, policy_version_id, config_digest, config)
        ON CONFLICT (
            system_id, derivation_id, target_store_path, policy_version_id,
            effective_set_digest
        ) DO UPDATE SET
            policy_lineage_id = EXCLUDED.policy_lineage_id,
            effective_config_digest = EXCLUDED.effective_config_digest,
            effective_config = EXCLUDED.effective_config,
            updated_at = CURRENT_TIMESTAMP
        RETURNING id, policy_version_id
        "#,
    )
    .bind(system_id)
    .bind(derivation_id)
    .bind(target_store_path)
    .bind(effective_set_digest)
    .bind(&lineage_ids)
    .bind(&version_ids)
    .bind(&config_digests)
    .bind(&configs)
    .fetch_all(&mut **tx)
    .await?;

    let mut assessment_ids = Vec::new();
    let mut rule_ids = Vec::new();
    let mut ordinals = Vec::new();
    let mut kinds = Vec::new();
    let mut phases = Vec::new();
    for (assessment_id, version_id) in &assessments {
        let policy = policies
            .iter()
            .find(|policy| policy.version_id == *version_id)
            .context("inserted assessment has no policy context")?;
        for (ordinal, rule) in policy.config.rules.iter().enumerate() {
            assessment_ids.push(*assessment_id);
            rule_ids.push(rule.id);
            ordinals.push(ordinal as i32);
            kinds.push(rule.rule.kind().to_string());
            phases.push(phase_str(expected_phase(&rule.rule)).to_string());
        }
    }
    sqlx::query(
        r#"
        INSERT INTO composite_policy_rule_results (
            assessment_id, rule_id, ordinal, kind, phase, outcome, blocking,
            detail, evidence
        )
        SELECT assessment_id, rule_id, ordinal, kind, phase, 'not_checked',
               TRUE, 'Phase has not completed', '{}'::jsonb
        FROM UNNEST($1::uuid[], $2::uuid[], $3::int[], $4::text[], $5::text[])
             AS input(assessment_id, rule_id, ordinal, kind, phase)
        ON CONFLICT (assessment_id, rule_id) DO NOTHING
        "#,
    )
    .bind(&assessment_ids)
    .bind(&rule_ids)
    .bind(&ordinals)
    .bind(&kinds)
    .bind(&phases)
    .execute(&mut **tx)
    .await?;
    Ok(assessments)
}

async fn bulk_merge_outcomes(
    tx: &mut Transaction<'_, Postgres>,
    rows: &[(Uuid, i32, CompositeRuleOutcome)],
    scan: Option<&LatestScan>,
) -> Result<()> {
    if rows.is_empty() {
        return Ok(());
    }
    let mut locked_assessment_ids = rows.iter().map(|row| row.0).collect::<Vec<_>>();
    locked_assessment_ids.sort_unstable();
    locked_assessment_ids.dedup();
    let system_ids: Vec<Uuid> = sqlx::query_scalar(
        r#"SELECT DISTINCT assessment.system_id
           FROM composite_policy_assessments assessment
           WHERE assessment.id=ANY($1)
           ORDER BY assessment.system_id"#,
    )
    .bind(&locked_assessment_ids)
    .fetch_all(&mut **tx)
    .await?;
    for system_id in system_ids {
        lock_poam_system_key_tx(tx, system_id).await?;
    }
    // Rule rows do not carry the stable finding key. Lock every affected system
    // sentinel and finding key before mutation so closure cannot observe a gap.
    sqlx::query(
        r#"SELECT lock_poam_finding_key(key.system_id,key.policy_lineage_id)
           FROM (
             SELECT DISTINCT assessment.system_id,assessment.policy_lineage_id
             FROM composite_policy_assessments assessment
             WHERE assessment.id=ANY($1)
             ORDER BY system_id,policy_lineage_id
           ) key"#,
    )
    .bind(&locked_assessment_ids)
    .execute(&mut **tx)
    .await?;
    let assessment_ids = rows.iter().map(|row| row.0).collect::<Vec<_>>();
    let ordinals = rows.iter().map(|row| row.1).collect::<Vec<_>>();
    let rule_ids = rows.iter().map(|row| row.2.rule_id).collect::<Vec<_>>();
    let kinds = rows
        .iter()
        .map(|row| row.2.kind.clone())
        .collect::<Vec<_>>();
    let phases = rows
        .iter()
        .map(|row| phase_str(row.2.phase).to_string())
        .collect::<Vec<_>>();
    let outcomes = rows
        .iter()
        .map(|row| outcome_str(row.2.outcome).to_string())
        .collect::<Vec<_>>();
    let blocking = rows.iter().map(|row| row.2.blocking).collect::<Vec<_>>();
    let details = rows
        .iter()
        .map(|row| row.2.detail.clone())
        .collect::<Vec<_>>();
    let evidence = rows
        .iter()
        .map(|row| row.2.evidence.clone())
        .collect::<Vec<_>>();
    let scan_ids = vec![scan.map(|scan| scan.id); rows.len()];
    let scan_orders = vec![scan.map(|scan| scan.composite_phase_order); rows.len()];
    let scan_derivation_ids = vec![scan.map(|scan| scan.derivation_id); rows.len()];
    sqlx::query(
        r#"
        INSERT INTO composite_policy_rule_results (
            assessment_id, rule_id, ordinal, kind, phase, outcome, blocking,
            detail, evidence, source_scan_id, source_scan_order, source_scan_derivation_id
        )
        SELECT *
        FROM UNNEST(
            $1::uuid[], $2::uuid[], $3::int[], $4::text[], $5::text[],
            $6::text[], $7::bool[], $8::text[], $9::jsonb[], $10::uuid[],
            $11::bigint[], $12::int[]
        )
        ON CONFLICT (assessment_id, rule_id) DO UPDATE SET
            ordinal = EXCLUDED.ordinal,
            kind = EXCLUDED.kind,
            phase = EXCLUDED.phase,
            outcome = EXCLUDED.outcome,
            blocking = EXCLUDED.blocking,
            detail = EXCLUDED.detail,
            evidence = EXCLUDED.evidence,
            source_scan_id = EXCLUDED.source_scan_id,
            source_scan_order = EXCLUDED.source_scan_order,
            source_scan_derivation_id = EXCLUDED.source_scan_derivation_id,
            evaluated_at = CURRENT_TIMESTAMP
        WHERE EXCLUDED.phase <> 'scan'
           OR composite_policy_rule_results.source_scan_order IS NULL
           OR composite_policy_rule_results.source_scan_order <= EXCLUDED.source_scan_order
        "#,
    )
    .bind(&assessment_ids)
    .bind(&rule_ids)
    .bind(&ordinals)
    .bind(&kinds)
    .bind(&phases)
    .bind(&outcomes)
    .bind(&blocking)
    .bind(&details)
    .bind(&evidence)
    .bind(&scan_ids)
    .bind(&scan_orders)
    .bind(&scan_derivation_ids)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn recompute_aggregates(
    tx: &mut Transaction<'_, Postgres>,
    assessment_ids: &[Uuid],
) -> Result<Vec<(Uuid, EnforcementOutcome)>> {
    if assessment_ids.is_empty() {
        return Ok(Vec::new());
    }
    let aggregates = sqlx::query_as::<_, (Uuid, String)>(
        r#"
        WITH aggregate AS (
            SELECT assessment_id,
                   CASE
                     WHEN BOOL_AND(outcome = 'pass') THEN 'pass'
                     WHEN BOOL_OR(outcome = 'error') THEN 'error'
                     WHEN BOOL_OR(outcome = 'fail') THEN 'fail'
                     ELSE 'not_checked'
                   END AS outcome
            FROM composite_policy_rule_results
            WHERE assessment_id = ANY($1)
            GROUP BY assessment_id
        ), updated AS (
            UPDATE composite_policy_assessments assessment
            SET overall_outcome = aggregate.outcome,
                updated_at = CURRENT_TIMESTAMP
            FROM aggregate
            WHERE assessment.id = aggregate.assessment_id
            RETURNING assessment.id, assessment.overall_outcome
        )
        SELECT id, overall_outcome FROM updated
        "#,
    )
    .bind(assessment_ids)
    .fetch_all(&mut **tx)
    .await?;
    Ok(aggregates
        .into_iter()
        .map(|(id, outcome)| (id, parse_outcome(&outcome)))
        .collect())
}

/// Persists the complete ordered assessment and evaluation-phase outcomes in
/// the caller's evaluation transaction. Assessment creation is intentionally
/// private to validated lifecycle hooks and final authorization never creates it.
///
/// # Errors
///
/// Returns an error when effective policies are invalid, the target does not
/// match the derivation, or PostgreSQL cannot persist the assessment state.
pub async fn persist_evaluation_assessments_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    system_id: Uuid,
    derivation_id: i32,
    target_store_path: &str,
    policy_results: &serde_json::Value,
    resolved: &EffectivePolicySet,
) -> Result<()> {
    let policies = policy_contexts(resolved)?;
    // CONCURRENCY: Assessment triggers acquire per-finding keys. Acquire the
    // system sentinel first so trigger locks preserve the global lock order.
    lock_poam_system_key_tx(tx, system_id).await?;
    sqlx::query(
        "DELETE FROM composite_policy_assessments WHERE system_id = $1 AND derivation_id = $2",
    )
    .bind(system_id)
    .bind(derivation_id)
    .execute(&mut **tx)
    .await?;
    let assessments = upsert_assessments_with_placeholders(
        tx,
        system_id,
        derivation_id,
        target_store_path,
        &resolved.effective_set_digest,
        &policies,
    )
    .await?;
    let mut rows = Vec::new();
    for (assessment_id, version_id) in &assessments {
        let policy = policies
            .iter()
            .find(|policy| policy.version_id == *version_id)
            .context("assessment policy context disappeared")?;
        for outcome in evaluation_outcomes(*version_id, &policy.config, policy_results) {
            let ordinal = policy
                .config
                .rules
                .iter()
                .position(|rule| rule.id == outcome.rule_id)
                .context("evaluation outcome references an unknown rule")?
                as i32;
            rows.push((*assessment_id, ordinal, outcome));
        }
    }
    bulk_merge_outcomes(tx, &rows, None).await?;
    let latest_scan = latest_scan_in_tx(tx, derivation_id).await?;
    if let Some(scan) = latest_scan.as_ref() {
        let mut scan_rows = Vec::new();
        for (assessment_id, version_id) in &assessments {
            let policy = policies
                .iter()
                .find(|policy| policy.version_id == *version_id)
                .context("assessment policy context disappeared")?;
            for (ordinal, rule) in policy.config.rules.iter().enumerate() {
                if matches!(rule.rule, CompositeRuleKind::CveBlock(_)) {
                    scan_rows.push((
                        *assessment_id,
                        ordinal as i32,
                        scan_outcome(rule.id, &rule.rule, Some(scan)),
                    ));
                }
            }
        }
        bulk_merge_outcomes(tx, &scan_rows, Some(scan)).await?;
    }
    let ids = assessments.iter().map(|row| row.0).collect::<Vec<_>>();
    recompute_aggregates(tx, &ids).await?;
    Ok(())
}

async fn latest_scan_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    derivation_id: i32,
) -> Result<Option<LatestScan>> {
    sqlx::query_as::<_, LatestScan>(
        r#"
        SELECT id, derivation_id, status, critical_count, high_count, medium_count, low_count,
               scan_metadata, created_at, completed_at, composite_phase_order
        FROM cve_scans
        WHERE derivation_id = $1
        ORDER BY composite_phase_order DESC
        LIMIT 1
        FOR SHARE
        "#,
    )
    .bind(derivation_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(Into::into)
}

/// Merges a scan transition only when it is newest for the exact derivation.
///
/// The source order guard makes delayed older completions a no-op.
///
/// # Errors
///
/// Returns an error when the transaction cannot load or merge the scan
/// outcome, or cannot commit the affected assessment aggregates.
pub async fn persist_scan_phase(pool: &PgPool, scan_id: Uuid) -> Result<()> {
    let mut tx = pool.begin().await?;
    persist_scan_phase_in_tx(&mut tx, scan_id).await?;
    tx.commit().await?;
    Ok(())
}

/// Locks all POA&M finding keys affected by one derivation.
///
/// Callers must retain the transaction until the related evidence mutation is
/// complete. The function uses the common derivation, system, and finding lock
/// order used by assessment and POA&M writers.
///
/// # Errors
///
/// Returns an error when PostgreSQL cannot resolve or acquire the advisory
/// lock.
pub(crate) async fn lock_poam_findings_for_derivation_tx(
    tx: &mut Transaction<'_, Postgres>,
    derivation_id: i32,
) -> Result<()> {
    lock_poam_derivation_key_tx(tx, derivation_id).await?;
    // CONCURRENCY: Legacy policies have no composite assessment rows. Include
    // findings for systems that currently deploy this derivation so legacy Nix
    // results and CVE scans use the same commit boundary as POA&M actions.
    let system_ids: Vec<Uuid> = sqlx::query_scalar(
        r#"SELECT key.system_id
           FROM (
             SELECT assessment.system_id
             FROM composite_policy_assessments assessment
             WHERE assessment.derivation_id=$1
             UNION
             SELECT system.id AS system_id
             FROM derivations derivation
             JOIN systems system ON true
             JOIN LATERAL (
               SELECT state.store_path FROM system_states state
               WHERE state.hostname=system.hostname AND state.store_path IS NOT NULL
                 AND btrim(state.store_path)<>''
               ORDER BY state.timestamp DESC,state.id DESC LIMIT 1
             ) deployed ON deployed.store_path=COALESCE(
               derivation.store_path,derivation.expected_store_path
             )
             WHERE derivation.id=$1
           ) key
           ORDER BY key.system_id"#,
    )
    .bind(derivation_id)
    .fetch_all(&mut **tx)
    .await?;
    for system_id in system_ids {
        lock_poam_system_key_tx(tx, system_id).await?;
    }
    sqlx::query(
        r#"SELECT lock_poam_finding_key(key.system_id,key.policy_lineage_id)
           FROM (
             SELECT assessment.system_id,assessment.policy_lineage_id
             FROM composite_policy_assessments assessment
             WHERE assessment.derivation_id=$1
             UNION
             SELECT finding.system_id,finding.policy_lineage_id
             FROM derivations derivation
             JOIN systems system ON true
             JOIN LATERAL (
               SELECT state.store_path FROM system_states state
               WHERE state.hostname=system.hostname AND state.store_path IS NOT NULL
                 AND btrim(state.store_path)<>''
               ORDER BY state.timestamp DESC,state.id DESC LIMIT 1
             ) deployed ON deployed.store_path=COALESCE(
               derivation.store_path,derivation.expected_store_path
             )
             JOIN poam_findings finding ON finding.system_id=system.id
             WHERE derivation.id=$1
             ORDER BY system_id,policy_lineage_id
           ) key"#,
    )
    .bind(derivation_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn lock_poam_derivation_key_tx(
    tx: &mut Transaction<'_, Postgres>,
    derivation_id: i32,
) -> Result<()> {
    sqlx::query("SELECT pg_advisory_xact_lock($1,$2)")
        .bind(POAM_DERIVATION_LOCK_NAMESPACE)
        .bind(derivation_id)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

/// Locks POA&M derivation keys that resolve to a deployed store path.
///
/// An absent or blank store path acquires no locks. Locks remain held for the
/// caller's transaction.
///
/// # Errors
///
/// Returns an error when PostgreSQL cannot resolve or acquire an advisory
/// lock.
pub(crate) async fn lock_poam_derivations_for_store_path_tx(
    tx: &mut Transaction<'_, Postgres>,
    store_path: Option<&str>,
) -> Result<()> {
    let Some(store_path) = store_path.filter(|path| !path.trim().is_empty()) else {
        return Ok(());
    };
    let derivation_ids: Vec<i32> = sqlx::query_scalar(
        r#"SELECT id FROM derivations
           WHERE derivation_type='nixos'
             AND COALESCE(store_path,expected_store_path)=$1
           ORDER BY id"#,
    )
    .bind(store_path)
    .fetch_all(&mut **tx)
    .await?;
    for derivation_id in derivation_ids {
        lock_poam_derivation_key_tx(tx, derivation_id).await?;
    }
    Ok(())
}

/// Locks the stable POA&M system sentinel for the caller's transaction.
///
/// Callers must acquire this sentinel before per-finding keys and row locks.
///
/// # Errors
///
/// Returns an error when PostgreSQL cannot acquire the advisory lock.
pub(crate) async fn lock_poam_system_key_tx(
    tx: &mut Transaction<'_, Postgres>,
    system_id: Uuid,
) -> Result<()> {
    // CONCURRENCY: The nil policy lineage reserves a stable system-level key.
    // Finding materialization, state publication, and POA&M actions acquire it
    // before per-finding keys so newly inserted findings cannot bypass a writer.
    sqlx::query("SELECT lock_poam_finding_key($1,$2)")
        .bind(system_id)
        .bind(Uuid::nil())
        .execute(&mut **tx)
        .await?;
    Ok(())
}

/// Locks the POA&M system sentinel and all existing finding keys for a system.
///
/// Locks remain held for the caller's transaction and follow the global writer
/// order used by deployment authorization and POA&M closure.
///
/// # Errors
///
/// Returns an error when PostgreSQL cannot enumerate or acquire the locks.
pub(crate) async fn lock_poam_findings_for_system_tx(
    tx: &mut Transaction<'_, Postgres>,
    system_id: Uuid,
) -> Result<()> {
    // CONCURRENCY: All callers acquire these stable finding keys before system,
    // assessment, or rule row locks. POA&M actions use the same key order, so an
    // action observes either the complete old state or the complete new state.
    lock_poam_system_key_tx(tx, system_id).await?;
    sqlx::query(
        r#"SELECT lock_poam_finding_key(key.system_id,key.policy_lineage_id)
           FROM (
             SELECT finding.system_id,finding.policy_lineage_id
             FROM poam_findings finding
             WHERE finding.system_id=$1
             ORDER BY finding.system_id,finding.policy_lineage_id
           ) key"#,
    )
    .bind(system_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// Persists one scan phase transition in the caller's transaction.
///
/// A transition for an older scan is a no-op. A newest scan updates the exact
/// derivation assessments and recomputes their aggregates before return.
///
/// # Errors
///
/// Returns an error when the scan is missing or its assessment state cannot be
/// decoded, locked, or persisted.
pub(crate) async fn persist_scan_phase_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    scan_id: Uuid,
) -> Result<()> {
    let scan = sqlx::query_as::<_, LatestScan>(
        r#"
        SELECT id, derivation_id, status, critical_count, high_count, medium_count, low_count,
               scan_metadata, created_at, completed_at, composite_phase_order
        FROM cve_scans WHERE id = $1 FOR SHARE
        "#,
    )
    .bind(scan_id)
    .fetch_optional(&mut **tx)
    .await?
    .context("CVE scan transition references a missing scan")?;
    let newest_id: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM cve_scans WHERE derivation_id = (SELECT derivation_id FROM cve_scans WHERE id = $1) ORDER BY composite_phase_order DESC LIMIT 1",
    )
    .bind(scan_id)
    .fetch_optional(&mut **tx)
    .await?;
    if newest_id != Some(scan.id) {
        return Ok(());
    }
    lock_poam_findings_for_derivation_tx(tx, scan.derivation_id).await?;
    let assessments = sqlx::query_as::<_, (Uuid, serde_json::Value)>(
        r#"
        SELECT assessment.id, assessment.effective_config
        FROM composite_policy_assessments assessment
        JOIN cve_scans scan ON scan.derivation_id = assessment.derivation_id
        WHERE scan.id = $1
        FOR UPDATE OF assessment
        "#,
    )
    .bind(scan_id)
    .fetch_all(&mut **tx)
    .await?;
    let mut rows = Vec::new();
    let mut assessment_ids = Vec::new();
    for (assessment_id, config_json) in assessments {
        let config = deserialize_policy_type_config("composite", &config_json)
            .map_err(anyhow::Error::msg)?
            .context("Persisted composite assessment config is invalid")?;
        assessment_ids.push(assessment_id);
        for (ordinal, rule) in config.rules.iter().enumerate() {
            if matches!(rule.rule, CompositeRuleKind::CveBlock(_)) {
                rows.push((
                    assessment_id,
                    ordinal as i32,
                    scan_outcome(rule.id, &rule.rule, Some(&scan)),
                ));
            }
        }
    }
    bulk_merge_outcomes(tx, &rows, Some(&scan)).await?;
    recompute_aggregates(tx, &assessment_ids).await?;
    Ok(())
}

async fn authorize_target_at(
    pool: &PgPool,
    system_id: Uuid,
    target: &str,
    now: DateTime<Utc>,
    action: AuthorizationAction<'_>,
) -> Result<TargetDeliveryAuthorization> {
    let constrained_derivation_id = match &action {
        AuthorizationAction::SetDesired {
            expected_derivation_id,
            ..
        } => *expected_derivation_id,
        AuthorizationAction::Check { .. } | AuthorizationAction::ClaimDelivery { .. } => None,
    };
    let mut tx = pool.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
        .execute(&mut *tx)
        .await?;
    // CONCURRENCY: This global lock precedes every POA&M, system, deployment,
    // and snapshot row lock used by deployment and agent-ingestion paths.
    crate::queries::evaluation_snapshots::lock_snapshot_writer_tx(&mut tx).await?;
    // Closure uses this same deterministic key order. Acquire it before the
    // system, assessment, or rule rows so neither path can invert lock order.
    lock_poam_system_key_tx(&mut tx, system_id).await?;
    sqlx::query(
        r#"SELECT lock_poam_finding_key(system_id,policy_lineage_id)
           FROM poam_findings WHERE system_id=$1
           ORDER BY system_id,policy_lineage_id"#,
    )
    .bind(system_id)
    .execute(&mut *tx)
    .await?;
    let current_desired = sqlx::query_scalar::<_, Option<String>>(
        "SELECT desired_target FROM systems WHERE id = $1 FOR UPDATE",
    )
    .bind(system_id)
    .fetch_optional(&mut *tx)
    .await?
    .context("Composite authorization system was not found")?;
    if let AuthorizationAction::ClaimDelivery { expected_target } = action {
        if current_desired.as_deref() != Some(expected_target) {
            tx.commit().await?;
            return Ok(TargetDeliveryAuthorization {
                target: None,
                authorization: CompositeAuthorization {
                    outcome: EnforcementOutcome::NotChecked,
                    assessments: Vec::new(),
                    detail: "Desired target changed before authorization".to_string(),
                },
            });
        }
    }

    let exact_target = sqlx::query_as::<_, (i32, String, i32, String)>(
        r#"
        SELECT d.id, d.store_path, c.id, d.derivation_name
        FROM systems s
        JOIN commits c ON c.flake_id = s.flake_id
        JOIN derivations d ON d.commit_id = c.id
        WHERE s.id = $1
          AND d.derivation_type = 'nixos'
          AND d.derivation_name = COALESCE(NULLIF(BTRIM(s.system_configuration_name), ''), s.hostname)
          AND d.store_path IS NOT NULL AND BTRIM(d.store_path) <> ''
          AND d.cf_agent_enabled IS TRUE
          AND d.policy_requirements_met IS TRUE
          AND d.error_message IS NULL
          AND EXISTS (
              SELECT 1
              FROM cache_push_jobs cache_push
              WHERE cache_push.derivation_id = d.id
                AND cache_push.status = 'completed'
                AND cache_push.store_path = d.store_path
          )
          AND ((LEFT($2, 11) = '/nix/store/' AND d.store_path = $2)
            OR (LEFT($2, 11) <> '/nix/store/' AND LOWER(c.git_commit_hash) = LOWER($2)))
          AND ($3::integer IS NULL OR d.id = $3)
        ORDER BY d.id DESC
        LIMIT 1
        FOR SHARE OF d, c
        "#,
    )
    .bind(system_id)
    .bind(target)
    .bind(constrained_derivation_id)
    .fetch_optional(&mut *tx)
    .await?;

    let resolved = match resolve_system_effective_policies_in_tx(&mut tx, system_id).await? {
        ResolutionOutcome::Resolved(resolved) => resolved,
        ResolutionOutcome::Conflict(conflicts) => bail!(
            "Effective policy resolution conflict: {}",
            conflicts
                .into_iter()
                .map(|conflict| conflict.message)
                .collect::<Vec<_>>()
                .join("; ")
        ),
    };
    let policies = policy_contexts(&resolved)?;
    let exact_target = match exact_target {
        Some(exact_target) => exact_target,
        None if policies.is_empty()
            && target.starts_with("/nix/store/")
            && !matches!(
                action,
                AuthorizationAction::Check {
                    expected_derivation_id: Some(_)
                }
            ) =>
        {
            let known_target_exists: bool = sqlx::query_scalar(
                r#"
                SELECT EXISTS (
                    SELECT 1
                    FROM systems system
                    JOIN commits commit ON commit.flake_id = system.flake_id
                    JOIN derivations derivation ON derivation.commit_id = commit.id
                    WHERE system.id = $1
                      AND derivation.derivation_type = 'nixos'
                      AND derivation.derivation_name = COALESCE(
                          NULLIF(BTRIM(system.system_configuration_name), ''),
                          system.hostname
                      )
                      AND derivation.store_path = $2
                )
                "#,
            )
            .bind(system_id)
            .bind(target)
            .fetch_one(&mut *tx)
            .await?;
            if known_target_exists {
                tx.commit().await?;
                return Ok(TargetDeliveryAuthorization {
                    target: None,
                    authorization: CompositeAuthorization {
                        outcome: EnforcementOutcome::Fail,
                        assessments: Vec::new(),
                        detail: "Known target is not deployable or has no completed cache push"
                            .to_string(),
                    },
                });
            }
            let policy_failed: bool = sqlx::query_scalar(
                r#"
                SELECT EXISTS (
                    SELECT 1
                    FROM derivations derivation
                    WHERE derivation.store_path = $1
                      AND derivation.derivation_type = 'nixos'
                      AND (derivation.cf_agent_enabled IS FALSE
                           OR derivation.policy_requirements_met IS FALSE)
                )
                "#,
            )
            .bind(target)
            .fetch_one(&mut *tx)
            .await?;
            if policy_failed {
                tx.commit().await?;
                return Ok(TargetDeliveryAuthorization {
                    target: None,
                    authorization: CompositeAuthorization {
                        outcome: EnforcementOutcome::Fail,
                        assessments: Vec::new(),
                        detail: "Historical target failed its recorded agent or policy checks"
                            .to_string(),
                    },
                });
            }
            let observed_for_system: bool = sqlx::query_scalar(
                r#"
                SELECT EXISTS (
                    SELECT 1
                    FROM systems system
                    JOIN system_states state ON state.hostname = system.hostname
                    WHERE system.id = $1
                      AND state.store_path = $2
                )
                "#,
            )
            .bind(system_id)
            .bind(target)
            .fetch_one(&mut *tx)
            .await?;
            if !observed_for_system {
                tx.commit().await?;
                return Ok(TargetDeliveryAuthorization {
                    target: None,
                    authorization: CompositeAuthorization {
                        outcome: EnforcementOutcome::Fail,
                        assessments: Vec::new(),
                        detail: "Target has no immutable observation for this system".to_string(),
                    },
                });
            }
            (-1, target.to_string(), -1, String::new())
        }
        None => {
            bail!("Composite authorization could not resolve exact system target {target:?}")
        }
    };
    if let AuthorizationAction::Check {
        expected_derivation_id: Some(expected_derivation_id),
    } = action
    {
        if exact_target.0 != expected_derivation_id {
            bail!("Exact deployment derivation changed before authorization");
        }
    }
    if !policies.is_empty() {
        let version_ids = policies
            .iter()
            .map(|policy| policy.version_id)
            .collect::<Vec<_>>();
        let locked_versions = sqlx::query_as::<_, (Uuid, Uuid, String)>(
            r#"
            SELECT version.id, version.policy_id, version.policy_type
            FROM deployment_policy_versions version
            JOIN deployment_policies policy ON policy.id = version.policy_id
            WHERE version.id = ANY($1)
            FOR SHARE OF version, policy
            "#,
        )
        .bind(&version_ids)
        .fetch_all(&mut *tx)
        .await?;
        if locked_versions.len() != policies.len()
            || policies.iter().any(|expected| {
                !locked_versions
                    .iter()
                    .any(|(version_id, lineage_id, policy_type)| {
                        *version_id == expected.version_id
                            && *lineage_id == expected.lineage_id
                            && policy_type == "composite"
                    })
            })
        {
            bail!("Effective composite policy versions changed before authorization");
        }
    }
    let mut assessment_ids = Vec::new();
    let mut configs_by_assessment = std::collections::HashMap::new();
    if !policies.is_empty() {
        let version_ids = policies.iter().map(|p| p.version_id).collect::<Vec<_>>();
        let assessments = sqlx::query_as::<_, (Uuid, Uuid, String, serde_json::Value)>(
            r#"
            SELECT id, policy_version_id, effective_config_digest, effective_config
            FROM composite_policy_assessments
            WHERE system_id = $1 AND derivation_id = $2 AND target_store_path = $3
              AND effective_set_digest = $4 AND policy_version_id = ANY($5)
            FOR UPDATE
            "#,
        )
        .bind(system_id)
        .bind(exact_target.0)
        .bind(&exact_target.1)
        .bind(&resolved.effective_set_digest)
        .bind(&version_ids)
        .fetch_all(&mut *tx)
        .await?;
        if assessments.len() != policies.len() {
            bail!("Exact current composite assessments are incomplete or stale");
        }
        for (assessment_id, version_id, digest, config_json) in assessments {
            let policy = policies
                .iter()
                .find(|policy| policy.version_id == version_id)
                .context("Assessment references a non-effective policy version")?;
            if digest != policy.config_digest || config_json != policy.config_json {
                bail!("Composite assessment effective config is stale");
            }
            assessment_ids.push(assessment_id);
            configs_by_assessment.insert(assessment_id, &policy.config);
        }
    }

    let newest_scan = latest_scan_in_tx(&mut tx, exact_target.0).await?;
    let persisted = sqlx::query_as::<_, PersistedRule>(
        r#"
        SELECT assessment_id, rule_id, source_scan_id, source_scan_order
        FROM composite_policy_rule_results
        WHERE assessment_id = ANY($1)
        ORDER BY assessment_id, ordinal
        FOR UPDATE
        "#,
    )
    .bind(&assessment_ids)
    .fetch_all(&mut *tx)
    .await?;
    let mut deployment_rows = Vec::new();
    for assessment_id in &assessment_ids {
        let config = configs_by_assessment[assessment_id];
        let rows = persisted
            .iter()
            .filter(|row| row.assessment_id == *assessment_id)
            .collect::<Vec<_>>();
        if rows.len() != config.rules.len() {
            bail!("Composite assessment is missing ordered rule placeholders");
        }
        for (ordinal, rule) in config.rules.iter().enumerate() {
            let row = rows
                .iter()
                .find(|row| row.rule_id == rule.id)
                .context("Composite assessment rule identity is stale")?;
            if matches!(rule.rule, CompositeRuleKind::CveBlock(_)) {
                let newest_matches = match newest_scan.as_ref() {
                    Some(scan) => {
                        row.source_scan_id == Some(scan.id)
                            && row.source_scan_order == Some(scan.composite_phase_order)
                    }
                    None => row.source_scan_id.is_none(),
                };
                if !newest_matches {
                    bail!("Composite scan outcome is not from the newest exact-derivation scan");
                }
            }
            if matches!(rule.rule, CompositeRuleKind::TimeWindow(_)) {
                deployment_rows.push((
                    *assessment_id,
                    ordinal as i32,
                    deployment_outcome(rule.id, &rule.rule, now),
                ));
            }
        }
    }
    bulk_merge_outcomes(&mut tx, &deployment_rows, None).await?;
    let aggregates = recompute_aggregates(&mut tx, &assessment_ids).await?;
    let outcome = if aggregates
        .iter()
        .all(|row| row.1 == EnforcementOutcome::Pass)
    {
        EnforcementOutcome::Pass
    } else if aggregates
        .iter()
        .any(|row| row.1 == EnforcementOutcome::Error)
    {
        EnforcementOutcome::Error
    } else if aggregates
        .iter()
        .any(|row| row.1 == EnforcementOutcome::Fail)
    {
        EnforcementOutcome::Fail
    } else {
        EnforcementOutcome::NotChecked
    };
    let authorization = CompositeAuthorization {
        outcome,
        assessments: assessment_ids,
        detail: format!(
            "{} composite policy assessment(s): {}",
            aggregates.len(),
            outcome_str(outcome)
        ),
    };
    let mut delivered_target = None;
    if authorization.allowed() {
        match action {
            AuthorizationAction::Check { .. } => {}
            AuthorizationAction::SetDesired {
                source,
                evaluation_snapshot_id,
                ..
            } => {
                let bound_snapshot_id: Option<Uuid> = if exact_target.0 == -1 {
                    None
                } else {
                    sqlx::query_scalar(
                        r#"
                    SELECT snapshot.id
                    FROM evaluation_snapshots snapshot
                    LEFT JOIN evaluation_snapshot_selections selection
                      ON selection.current_snapshot_id = snapshot.id
                     AND selection.commit_id = snapshot.commit_id
                     AND selection.configuration_name = snapshot.configuration_name
                    WHERE snapshot.commit_id = $1
                      AND snapshot.configuration_name = $2
                       AND snapshot.lifecycle = 'available'
                       AND snapshot.integrity_version = 1
                      AND (($3::uuid IS NOT NULL AND snapshot.id = $3)
                        OR ($3::uuid IS NULL AND selection.current_snapshot_id IS NOT NULL))
                    LIMIT 1
                    "#,
                    )
                    .bind(exact_target.2)
                    .bind(&exact_target.3)
                    .bind(evaluation_snapshot_id)
                    .fetch_optional(&mut *tx)
                    .await?
                };
                if evaluation_snapshot_id.is_some() && bound_snapshot_id != evaluation_snapshot_id {
                    bail!("Retained deployment artifact does not match the exact target lineage");
                }
                sqlx::query(
                    "UPDATE systems SET desired_target = $1, desired_target_set_at = NOW(), updated_at = NOW() WHERE id = $2",
                )
                .bind(&exact_target.1)
                .bind(system_id)
                .execute(&mut *tx)
                .await?;
                // IDENTITY: Equal store paths can come from different commits.
                // End incompatible pending work before the generic target helper
                // performs its path-based deduplication.
                if exact_target.0 != -1 {
                    sqlx::query(
                        "UPDATE pending_system_deployments
                         SET status = 'superseded', completed_at = NOW()
                         WHERE system_id = $1 AND target_store_path = $2 AND status = 'pending'
                           AND (requested_commit_id IS DISTINCT FROM $3
                             OR requested_derivation_id IS DISTINCT FROM $5
                             OR ($4::uuid IS NOT NULL
                                AND evaluation_snapshot_id IS DISTINCT FROM $4))",
                    )
                    .bind(system_id)
                    .bind(&exact_target.1)
                    .bind(exact_target.2)
                    .bind(bound_snapshot_id)
                    .bind(exact_target.0)
                    .execute(&mut *tx)
                    .await?;
                }
                let deployment_id = set_pending_deployment_target_tx(
                    &mut tx,
                    system_id,
                    Some(&exact_target.1),
                    source,
                )
                .await?
                .context("authorized deployment target did not create or reuse pending work")?;
                if exact_target.0 != -1 {
                    let bound = sqlx::query(
                        "UPDATE pending_system_deployments
                         SET requested_commit_id = $2, evaluation_snapshot_id = $3,
                             requested_derivation_id = $4
                         WHERE id = $1
                           AND (requested_commit_id IS NULL OR requested_commit_id = $2)
                           AND (evaluation_snapshot_id IS NULL OR evaluation_snapshot_id = $3)
                           AND (requested_derivation_id IS NULL OR requested_derivation_id = $4)",
                    )
                    .bind(deployment_id)
                    .bind(exact_target.2)
                    .bind(bound_snapshot_id)
                    .bind(exact_target.0)
                    .execute(&mut *tx)
                    .await?;
                    if bound.rows_affected() != 1 {
                        bail!("Pending deployment already has different immutable lineage");
                    }
                    crate::queries::evaluation_snapshots::retain_bound_deployment_observations_tx(
                        &mut tx,
                        deployment_id,
                    )
                    .await?;
                }
            }
            AuthorizationAction::ClaimDelivery { expected_target } => {
                // Upgrade bridge: only targets captured by migration may gain a
                // pending row here, and only after this exact target passed the
                // current serializable authorization check. Newly issued targets
                // still require the normal set-time pending-row CAS contract.
                sqlx::query(
                    r#"
                    WITH legacy AS (
                        DELETE FROM composite_legacy_desired_targets
                        WHERE system_id = $1 AND target_store_path = $2
                        RETURNING system_id, target_store_path
                    )
                    INSERT INTO pending_system_deployments (
                        system_id, target_store_path, source, expires_at, metadata
                    )
                    SELECT legacy.system_id, legacy.target_store_path,
                           'legacy_authorized_desired_target', NOW() + INTERVAL '2 hours',
                           jsonb_build_object('desired_target', legacy.target_store_path,
                                              'upgrade_authorized', true)
                    FROM legacy
                    WHERE NOT EXISTS (
                        SELECT 1 FROM pending_system_deployments pending
                        WHERE pending.system_id = legacy.system_id
                          AND pending.target_store_path = legacy.target_store_path
                          AND pending.status = 'pending' AND pending.expires_at > NOW()
                    )
                    "#,
                )
                .bind(system_id)
                .bind(expected_target)
                .execute(&mut *tx)
                .await?;
                let claimed = sqlx::query(
                    r#"
                    UPDATE pending_system_deployments pending
                    SET delivered_at = COALESCE(delivered_at, NOW())
                    FROM systems system
                    WHERE pending.system_id = $1
                      AND pending.target_store_path = $2
                      AND pending.status = 'pending'
                      AND pending.expires_at > NOW()
                      AND system.id = pending.system_id
                      AND system.desired_target = $2
                    "#,
                )
                .bind(system_id)
                .bind(expected_target)
                .execute(&mut *tx)
                .await?;
                if claimed.rows_affected() > 0 {
                    delivered_target = Some(expected_target.to_string());
                }
            }
        }
    }
    tx.commit().await?;
    Ok(TargetDeliveryAuthorization {
        target: delivered_target,
        authorization,
    })
}

/// Authorizes an exact target for a system without changing desired state.
///
/// # Errors
///
/// Returns an error when policy resolution conflicts, target evidence is
/// missing or stale, or PostgreSQL cannot complete authorization.
pub async fn authorize_system_target(
    pool: &PgPool,
    system_id: Uuid,
    target: &str,
) -> Result<CompositeAuthorization> {
    Ok(authorize_target_at(
        pool,
        system_id,
        target,
        Utc::now(),
        AuthorizationAction::Check {
            expected_derivation_id: None,
        },
    )
    .await?
    .authorization)
}

/// Authorizes and sets a system's desired target in one transaction.
///
/// The desired target changes only when the aggregate authorization passes.
///
/// # Errors
///
/// Returns an error when policy resolution or target validation fails, or when
/// PostgreSQL cannot complete the guarded target update.
pub async fn authorize_and_set_system_target(
    pool: &PgPool,
    system_id: Uuid,
    target: &str,
    source: &str,
) -> Result<CompositeAuthorization> {
    Ok(authorize_target_at(
        pool,
        system_id,
        target,
        Utc::now(),
        AuthorizationAction::SetDesired {
            source,
            evaluation_snapshot_id: None,
            expected_derivation_id: None,
        },
    )
    .await?
    .authorization)
}

/// Authorizes and sets a desired target with an exact retained artifact.
///
/// The artifact and derivation must match the retained NixOS lineage. The
/// pending deployment stores the immutable artifact and commit identities so
/// later agent generation ingestion does not consult the current selector.
///
/// # Errors
///
/// Returns an error when authorization fails, the artifact lineage differs
/// from the target, or PostgreSQL cannot commit the guarded update.
pub async fn authorize_and_set_system_target_with_artifact(
    pool: &PgPool,
    system_id: Uuid,
    target: &str,
    source: &str,
    evaluation_snapshot_id: Uuid,
    derivation_id: i32,
) -> Result<CompositeAuthorization> {
    Ok(authorize_target_at(
        pool,
        system_id,
        target,
        Utc::now(),
        AuthorizationAction::SetDesired {
            source,
            evaluation_snapshot_id: Some(evaluation_snapshot_id),
            expected_derivation_id: Some(derivation_id),
        },
    )
    .await?
    .authorization)
}

/// Authorizes and claims delivery of the expected desired target atomically.
///
/// A changed desired target or non-passing decision returns no claimed target.
///
/// # Errors
///
/// Returns an error when policy resolution or target validation fails, or when
/// PostgreSQL cannot complete the delivery claim.
pub async fn authorize_and_claim_desired_target(
    pool: &PgPool,
    system_id: Uuid,
    expected_target: &str,
) -> Result<TargetDeliveryAuthorization> {
    authorize_target_at(
        pool,
        system_id,
        expected_target,
        Utc::now(),
        AuthorizationAction::ClaimDelivery { expected_target },
    )
    .await
}

/// Authorizes an exact deployment at the supplied evaluation timestamp.
///
/// The derivation ID and store path must identify the same deployable target.
///
/// # Errors
///
/// Returns an error when exact target evidence is incomplete or stale, policy
/// resolution conflicts, or PostgreSQL cannot complete authorization.
pub async fn authorize_deployment_at(
    pool: &PgPool,
    system_id: Uuid,
    derivation_id: i32,
    target_store_path: &str,
    now: DateTime<Utc>,
) -> Result<CompositeAuthorization> {
    Ok(authorize_target_at(
        pool,
        system_id,
        target_store_path,
        now,
        AuthorizationAction::Check {
            expected_derivation_id: Some(derivation_id),
        },
    )
    .await?
    .authorization)
}

/// Authorizes an exact deployment at the current UTC time.
///
/// # Errors
///
/// Returns an error under the same conditions as [`authorize_deployment_at`].
pub async fn authorize_deployment(
    pool: &PgPool,
    system_id: Uuid,
    derivation_id: i32,
    target_store_path: &str,
) -> Result<CompositeAuthorization> {
    authorize_deployment_at(
        pool,
        system_id,
        derivation_id,
        target_store_path,
        Utc::now(),
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::deployment_policies::{CveBlockRuleConfig, TimeWindowRuleConfig};
    use chrono::TimeZone;

    fn scan(status: &str, critical_count: i32) -> LatestScan {
        LatestScan {
            id: Uuid::from_u128(1),
            derivation_id: 1,
            status: status.to_string(),
            critical_count,
            high_count: 0,
            medium_count: 0,
            low_count: 0,
            scan_metadata: Some(serde_json::json!({"error": "scanner failed"})),
            created_at: Some(Utc.timestamp_opt(1_700_000_000, 0).single().unwrap()),
            completed_at: Some(Utc.timestamp_opt(1_700_000_100, 0).single().unwrap()),
            composite_phase_order: 1,
        }
    }

    #[test]
    fn cve_block_maps_missing_active_failed_and_completed_attempts() {
        let rule = CompositeRuleKind::CveBlock(CveBlockRuleConfig {
            severity: CveBlockSeverity::Critical,
            max_allowed: 0,
        });
        let rule_id = Uuid::from_u128(2);
        for (scan, expected) in [
            (None, EnforcementOutcome::NotChecked),
            (Some(scan("in_progress", 0)), EnforcementOutcome::NotChecked),
            (Some(scan("failed", 0)), EnforcementOutcome::Error),
            (Some(scan("completed", 0)), EnforcementOutcome::Pass),
            (Some(scan("completed", 1)), EnforcementOutcome::Fail),
        ] {
            let outcome = scan_outcome(rule_id, &rule, scan.as_ref());
            assert_eq!(outcome.rule_id, rule_id);
            assert_eq!(outcome.kind, "cve_block");
            assert_eq!(outcome.phase, EnforcementPhase::Scan);
            assert_eq!(outcome.outcome, expected);
            assert_eq!(outcome.blocking, expected != EnforcementOutcome::Pass);
            assert!(outcome.evidence.is_object());
            match scan.as_ref().map(|scan| scan.status.as_str()) {
                Some("completed") => {
                    assert_eq!(outcome.evidence["severity"], "critical");
                    assert_eq!(outcome.evidence["max_allowed"], 0);
                    assert!(outcome.evidence["scan_id"].as_str().is_some());
                }
                Some(_) => assert!(outcome.evidence["scan_id"].as_str().is_some()),
                None => assert_eq!(outcome.evidence, serde_json::json!({})),
            }
        }
    }

    #[test]
    fn time_window_uses_injected_utc_time_and_records_timezone_evidence() {
        let rule = CompositeRuleKind::TimeWindow(TimeWindowRuleConfig {
            days: vec!["mon".to_string()],
            from: "09:00".to_string(),
            to: "17:00".to_string(),
            tz: "UTC".to_string(),
        });
        let inside = Utc.with_ymd_and_hms(2026, 8, 24, 12, 0, 0).unwrap();
        let outside = Utc.with_ymd_and_hms(2026, 8, 25, 12, 0, 0).unwrap();
        for (at, expected) in [
            (inside, EnforcementOutcome::Pass),
            (outside, EnforcementOutcome::Fail),
        ] {
            let outcome = deployment_outcome(Uuid::from_u128(3), &rule, at);
            assert_eq!(outcome.kind, "time_window");
            assert_eq!(outcome.phase, EnforcementPhase::Deployment);
            assert_eq!(outcome.outcome, expected);
            assert_eq!(outcome.blocking, expected != EnforcementOutcome::Pass);
            assert_eq!(outcome.evidence["timezone"], "UTC");
            assert!(outcome.evidence["evaluated_at"].as_str().is_some());
        }
    }
}
