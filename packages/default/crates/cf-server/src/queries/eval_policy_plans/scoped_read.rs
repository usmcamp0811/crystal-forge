//! Current-environment authorization and metadata projection for matrix reads.

use super::*;
use std::collections::HashSet;

/// Identifies a currently authorized host and its evaluator configuration.
#[derive(sqlx::FromRow)]
pub(super) struct VisibleSystem {
    id: Uuid,
    configuration_name: String,
}

// SECURITY: The outer Option hides missing and foreign commits identically.
// The inner None is an admin scope. All checks use the caller's data snapshot;
// no historic assignment or environment identifier can authorize a system.
/// Returns no scope for missing or foreign commits and an inner None for admins.
///
/// # Errors
/// Returns database query errors without reading captured evidence.
pub(super) async fn authorize(
    tx: &mut Transaction<'_, Postgres>,
    commit_id: i32,
    user_id: Option<Uuid>,
) -> Result<Option<Option<Vec<VisibleSystem>>>> {
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM commits WHERE id = $1)")
        .bind(commit_id)
        .fetch_one(&mut **tx)
        .await?;
    if !exists {
        return Ok(None);
    }
    let Some(user_id) = user_id else {
        return Ok(Some(None));
    };
    // Match the loader's configuration normalization before comparing the
    // current host with its immutable captured identity.
    let systems = sqlx::query_as::<_, VisibleSystem>(
        "SELECT s.id,
                COALESCE(NULLIF(BTRIM(s.system_configuration_name), ''), s.hostname) AS configuration_name
         FROM systems s JOIN commits c ON c.flake_id = s.flake_id
         WHERE c.id = $1 AND EXISTS (
             SELECT 1 FROM user_environment_memberships m
             WHERE m.user_id = $2 AND m.environment_id = s.environment_id)",
    )
    .bind(commit_id)
    .bind(user_id)
    .fetch_all(&mut **tx)
    .await?;
    Ok((!systems.is_empty()).then_some(Some(systems)))
}

/// Prunes captured identities and metadata to current authorized hosts.
pub(super) fn project(
    systems: &[VisibleSystem],
    plan: &mut EvalPolicyMatrixSnapshot,
    outcomes: &mut Vec<EvalPolicySystemRow>,
) {
    // SECURITY: A captured host must retain its current identity, flake and
    // configuration. A replacement host with the same name cannot inherit it.
    // Legacy rows lack that proof, so restricted readers receive no legacy
    // evidence. Administrators retain the existing compatibility projection.
    plan.rows.retain(|row| {
        row.system_id.is_some_and(|id| {
            systems.iter().any(|system| {
                system.id == id && system.configuration_name == row.configuration_name
            })
        })
    });
    let policies: HashSet<_> = plan
        .rows
        .iter()
        .flat_map(|row| row.policies.iter().map(|policy| policy.policy_version_id))
        .collect();
    let bundles: HashSet<_> = plan
        .rows
        .iter()
        .flat_map(|row| {
            row.assigned_bundles
                .iter()
                .chain(
                    row.policies
                        .iter()
                        .flat_map(|policy| policy.memberships.iter()),
                )
                .map(|membership| membership.bundle_version_id)
        })
        .collect();
    plan.columns
        .retain(|column| policies.contains(&column.policy_version_id));
    plan.bundles
        .retain(|bundle| bundles.contains(&bundle.bundle_version_id));
    outcomes.retain(|outcome| {
        plan.rows
            .iter()
            .any(|row| row.configuration_name == outcome.system_name)
    });
}

/// Projects shared outcomes to visible host identities and policy bindings.
pub(super) fn project_flat(
    plan: &EvalPolicyMatrixSnapshot,
    outcomes: &mut Vec<EvalPolicySystemRow>,
) {
    // The compatibility matrix is derived from per-host visible bindings, never
    // from the configuration-wide union. This also excludes hidden-only policy
    // labels and details when visible and hidden hosts share one evaluator.
    let mut flat = Vec::new();
    for row in &plan.rows {
        let allowed: HashSet<_> = row
            .policies
            .iter()
            .map(|policy| policy.policy_version_id.to_string())
            .collect();
        for outcome in outcomes
            .iter()
            .filter(|o| o.system_name == row.configuration_name)
        {
            let mut outcome = outcome.clone();
            outcome.system_name = row.system_name.clone();
            if let Some(assigned) = outcome
                .policy_results
                .get_mut("assigned")
                .and_then(Value::as_object_mut)
            {
                assigned.retain(|id, _| allowed.contains(id));
            }
            flat.push(outcome);
        }
    }
    *outcomes = flat;
}
