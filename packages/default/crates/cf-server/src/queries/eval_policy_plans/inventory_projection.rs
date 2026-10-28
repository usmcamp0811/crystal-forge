//! Reconciles immutable discovery with immutable per-host policy provenance.

use super::*;
use std::collections::{HashMap, HashSet};

/// Adds visible discovered identities without assigning invented system/policy IDs.
///
/// # Errors
/// Returns database errors from the caller's consistent read transaction.
pub(super) async fn extend(
    tx: &mut Transaction<'_, Postgres>,
    commit_id: i32,
    user_id: Option<Uuid>,
    historical_identities: &[(Option<Uuid>, String)],
    plan: &mut EvalPolicyMatrixSnapshot,
    outcomes: &mut [EvalPolicySystemRow],
) -> Result<()> {
    let inventory: Option<(Vec<String>, Vec<String>, String, bool)> = sqlx::query_as(
        "SELECT i.configuration_names,i.selected_configuration_names,i.execution_mode,
            (NOT EXISTS (SELECT 1 FROM evaluation_attempts newer WHERE newer.commit_id=a.commit_id
                AND newer.attempt_number>a.attempt_number)
             AND NOT COALESCE(c.cancellation_requested,false)
             AND (a.status='queued' OR (a.status='in_progress'
                AND c.evaluation_attempt_count=a.attempt_number AND c.evaluation_status='in_progress')))
         FROM evaluation_attempt_inventory i JOIN evaluation_attempts a ON a.id=i.evaluation_attempt_id
         JOIN commits c ON c.id=i.commit_id
         WHERE i.commit_id=$1 AND i.evaluation_attempt_id=$2 AND i.attempt_number=$3",
    )
    .bind(commit_id)
    .bind(plan.evaluation_attempt_id)
    .bind(plan.attempt_number)
    .fetch_optional(&mut **tx)
    .await?;
    if plan.provenance == EvalMatrixProvenance::LegacyUnavailable {
        // Outcome rows do not prove inventory. Without a plan, only the exact
        // discovery extension can establish a configuration's matrix identity.
        plan.rows.clear();
    }
    let Some((mut discovered, mut selected, mode, can_complete)) = inventory else {
        if user_id.is_some() {
            // SECURITY: Without discovery, current host registration cannot
            // prove complete authority over configuration-wide legacy facts.
            for outcome in outcomes {
                redact_configuration_gate(outcome);
            }
        }
        return Ok(());
    };
    let mut wholly_visible = None;
    if let Some(user_id) = user_id {
        let hosts: Vec<(Uuid, String, bool)> = sqlx::query_as(
            "SELECT s.id, COALESCE(NULLIF(BTRIM(s.system_configuration_name), ''),s.hostname),
                 EXISTS(SELECT 1 FROM user_environment_memberships m
                     WHERE m.user_id=$2 AND m.environment_id=s.environment_id)
             FROM systems s JOIN commits c ON c.flake_id=s.flake_id WHERE c.id=$1",
        )
        .bind(commit_id)
        .bind(user_id)
        .fetch_all(&mut **tx)
        .await?;
        // SECURITY: A newly registered/replacement host cannot turn guessed
        // names into a historical discovery oracle. Only retained captured
        // UUID/configuration pairs authorize names for nonadmins.
        let visible: HashSet<_> = plan
            .rows
            .iter()
            .filter(|row| row.system_id.is_some())
            .map(|row| &row.configuration_name)
            .collect();
        discovered.retain(|name| visible.contains(name));
        selected.retain(|name| visible.contains(name));
        // SECURITY: Configuration-wide gates can include requirements from any
        // host sharing the evaluator. Partial ownership cannot reveal that gate.
        wholly_visible = Some(
            discovered
                .iter()
                .filter(|name| {
                    !hosts
                        .iter()
                        .any(|(_, host, visible)| host == *name && !visible)
                        && historical_identities
                            .iter()
                            .any(|(_, config)| config == *name)
                        && historical_identities
                            .iter()
                            .filter(|(_, config)| config == *name)
                            .all(|(id, config)| {
                                id.is_some_and(|id| {
                                    hosts.iter().any(|(host_id, current, visible)| {
                                        *host_id == id && current == config && *visible
                                    })
                                })
                            })
                })
                .cloned()
                .collect::<HashSet<_>>(),
        );
    }
    for name in &discovered {
        if !plan.rows.iter().any(|row| &row.configuration_name == name) {
            plan.rows.push(EvalMatrixRow {
                greenfield: None,
                system_id: None,
                system_name: name.clone(),
                configuration_name: name.clone(),
                resolution: EvalMatrixResolution::Unavailable,
                evaluation_digest: None,
                assigned_bundles: Vec::new(),
                policies: Vec::new(),
                cells: Vec::new(),
                details: Vec::new(),
            });
        }
    }
    let mut by_name = HashMap::<&str, Vec<&EvalPolicySystemRow>>::new();
    for row in outcomes.iter().filter(|row| {
        matches!(row.eval_status.as_str(), "evaluated" | "eval_failed")
            && row
                .policy_results
                .get("evaluation_attempt")
                .and_then(Value::as_i64)
                == Some(i64::from(plan.attempt_number))
    }) {
        by_name
            .entry(row.system_name.as_str())
            .or_default()
            .push(row);
    }
    let mut captures_by_name = HashMap::<&str, Vec<&EvalMatrixRow>>::new();
    for row in &plan.rows {
        captures_by_name
            .entry(row.configuration_name.as_str())
            .or_default()
            .push(row);
    }
    let ambiguous: HashSet<_> = outcomes
        .iter()
        .filter(|row| row.eval_status == "ambiguous")
        .map(|row| row.system_name.as_str())
        .collect();
    // The primary population is the union of captured planned configurations
    // and verified discovery. A planned configuration absent from discovery is
    // retained as unavailable coverage, never counted as executed or excluded.
    let population = discovered
        .iter()
        .cloned()
        .chain(plan.rows.iter().map(|row| row.configuration_name.clone()))
        .collect::<HashSet<_>>();
    let coverage = selected
        .iter()
        .cloned()
        .chain(
            population
                .iter()
                .filter(|name| !discovered.contains(name))
                .cloned(),
        )
        .collect::<HashSet<_>>();
    let mut summary = EvalConfigurationSummary {
        total: population.len(),
        excluded: discovered.len().saturating_sub(selected.len()),
        ..Default::default()
    };
    for name in &coverage {
        if !selected.contains(name) {
            summary.unavailable += 1;
            continue;
        }
        if ambiguous.contains(name.as_str())
            || by_name
                .get(name.as_str())
                .is_some_and(|rows| rows.len() > 1)
        {
            summary.unavailable += 1;
            continue;
        }
        let requirements_visible = wholly_visible
            .as_ref()
            .is_none_or(|names| names.contains(name));
        let evidence = by_name
            .get(name.as_str())
            .filter(|rows| rows.len() == 1)
            .and_then(|rows| rows.first())
            .copied()
            .filter(|row| {
                row.policy_results
                    .get("evaluation_attempt")
                    .and_then(Value::as_i64)
                    == Some(i64::from(plan.attempt_number))
            });
        let Some(row) = evidence else {
            if can_complete {
                summary.pending += 1;
            } else {
                summary.unavailable += 1;
            }
            continue;
        };
        if row.eval_status == "eval_failed" {
            summary.evaluation_failed += 1;
        } else if row.eval_status == "evaluated" {
            let agent = row
                .policy_results
                .get("global")
                .and_then(|g| g.get("cfAgentEnabled"));
            // Use the same saved gates as queue/history. Display JSON must not
            // become a second definition of configuration status.
            let requirements = if requirements_visible {
                row.policy_requirements_met
            } else {
                None
            };
            match (row.cf_agent_enabled, requirements) {
                (Some(true), Some(true)) => summary.passed += 1,
                (Some(false), _) | (_, Some(false)) => {
                    summary.failed += 1;
                    if row.cf_agent_enabled == Some(false) {
                        // Only saved, exact-attempt failure text is exposed. A
                        // missing diagnostic does not manufacture an assertion.
                        plan.configuration_failures.push(EvalConfigurationFailure {
                            kind: EvalConfigurationFailureKind::CfAgent,
                            configuration_name: name.clone(),
                            // The intrinsic gate belongs to this configuration.
                            // Partial ownership still withholds free-form text.
                            detail: if requirements_visible {
                                agent
                                    .and_then(|a| a.get("details"))
                                    .and_then(Value::as_str)
                                    .unwrap_or("")
                                    .to_owned()
                            } else {
                                String::new()
                            },
                        });
                    } else {
                        let control_failure = captures_by_name
                            .get(name.as_str())
                            .into_iter()
                            .flat_map(|rows| rows.iter())
                            .any(|captured| {
                                captured.policies.iter().any(|binding| {
                                    super::classify_cell(
                                        captured.resolution,
                                        Some(binding),
                                        Some(row),
                                        plan.attempt_number,
                                    ) == EvalMatrixCell::Fail
                                })
                            });
                        if !control_failure {
                            plan.configuration_failures.push(EvalConfigurationFailure {
                                kind: EvalConfigurationFailureKind::Requirements,
                                configuration_name: name.clone(),
                                detail: String::new(),
                            });
                        }
                    }
                }
                _ => summary.unavailable += 1,
            }
        } else {
            summary.unavailable += 1;
        }
    }
    // Stable ordering lets clients retain no-op snapshot memoization on polls.
    plan.configuration_failures
        .sort_by(|a, b| a.configuration_name.cmp(&b.configuration_name));
    plan.inventory = Some(EvalAttemptInventory {
        execution_mode: mode,
        configurations: discovered,
        selected_configurations: selected,
    });
    plan.configuration_summary = Some(summary);
    if let Some(allowed) = wholly_visible {
        for outcome in outcomes {
            // SECURITY: Nix stderr may describe other configurations in the
            // flake, even when this configuration's gate is wholly authorized.
            // Whole-flake log access, not a scoped matrix, grants that text.
            outcome.error_message = None;
            if let Some(document) = outcome.policy_results.as_object_mut() {
                document.remove("evaluation_failure_error");
            }
            if !allowed.contains(&outcome.system_name) {
                // SECURITY: Per-host assigned checks are projected separately.
                // Global gates and free-form diagnostics can include hidden
                // shared-host facts and must not survive the flat projection.
                redact_configuration_gate(outcome);
            }
        }
    }
    Ok(())
}

fn redact_configuration_gate(outcome: &mut EvalPolicySystemRow) {
    if let Some(document) = outcome.policy_results.as_object_mut() {
        document.remove("global");
        document.remove("evaluation_failure_error");
        document.remove("evaluation_failure");
    }
    outcome.policy_requirements_met = None;
    outcome.cf_agent_enabled = None;
    outcome.error_message = None;
    if outcome.eval_status == "eval_failed" {
        outcome.eval_status = "unavailable".into();
    }
}

/// Masks execution evidence outside verified selection without changing policies.
pub(super) fn mask_outside_selection(plan: &mut EvalPolicyMatrixSnapshot) {
    let selected = plan.inventory.as_ref().map(|i| &i.selected_configurations);
    for row in &mut plan.rows {
        if selected.is_none_or(|names| !names.contains(&row.configuration_name)) {
            row.cells.fill(EvalMatrixCell::Unknown);
            row.details.fill(None);
        }
    }
}
