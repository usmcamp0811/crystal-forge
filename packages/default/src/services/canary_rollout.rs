// Canary rollout orchestration service
// Manages phased deployment to fleet subsets with observation periods

use chrono::{DateTime, Duration, Utc};
use sqlx::PgPool;
use std::collections::HashSet;
use uuid::Uuid;

use crate::models::deployment_policies::CanaryConfig;

/// Rollout context types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RolloutContext {
    Commit,
    Derivation,
}

impl RolloutContext {
    pub fn as_str(&self) -> &'static str {
        match self {
            RolloutContext::Commit => "commit",
            RolloutContext::Derivation => "derivation",
        }
    }
}

/// Rollout status
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RolloutStatus {
    InProgress,
    Observing,
    Completed,
    Failed,
    Halted,
}

impl RolloutStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            RolloutStatus::InProgress => "in_progress",
            RolloutStatus::Observing => "observing",
            RolloutStatus::Completed => "completed",
            RolloutStatus::Failed => "failed",
            RolloutStatus::Halted => "halted",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "in_progress" => RolloutStatus::InProgress,
            "observing" => RolloutStatus::Observing,
            "completed" => RolloutStatus::Completed,
            "failed" => RolloutStatus::Failed,
            "halted" => RolloutStatus::Halted,
            _ => RolloutStatus::InProgress,
        }
    }
}

/// Current state of a canary rollout
#[derive(Debug, Clone)]
pub struct RolloutState {
    pub id: Uuid,
    pub current_phase: i32,
    pub total_phases: i32,
    pub status: RolloutStatus,
    pub phase_started_at: DateTime<Utc>,
    pub phase_observation_end: Option<DateTime<Utc>>,
    pub systems_in_current_phase: Vec<Uuid>,
    pub systems_completed: Vec<Uuid>,
    pub systems_failed: Vec<Uuid>,
    pub halted_reason: Option<String>,
}

/// Result of canary rollout evaluation
#[derive(Debug, Clone)]
pub struct CanaryResult {
    pub deployment_allowed: bool,
    pub systems_to_deploy: Vec<Uuid>,
    pub reason: Option<String>,
    pub rollout_state: Option<RolloutState>,
}

/// Initialize a new canary rollout
pub async fn init_rollout(
    pool: &PgPool,
    context: RolloutContext,
    context_id: &str,
    policy_id: Uuid,
    config: &CanaryConfig,
    all_systems: &[Uuid],
) -> Result<RolloutState, sqlx::Error> {
    let total_phases = calculate_total_phases(config.percentage);
    let first_phase_systems = select_systems_for_phase(
        all_systems,
        &[],
        &[],
        config.percentage,
        &config.selection_strategy,
        1,
    );

    let state_id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO canary_rollout_state (
            rollout_context_type,
            rollout_context_id,
            policy_id,
            current_phase,
            total_phases,
            systems_in_current_phase,
            status
        )
        VALUES ($1, $2, $3, 1, $4, $5, 'in_progress')
        RETURNING id
        "#,
    )
    .bind(context.as_str())
    .bind(context_id)
    .bind(policy_id)
    .bind(total_phases)
    .bind(&first_phase_systems)
    .fetch_one(pool)
    .await?;

    Ok(RolloutState {
        id: state_id,
        current_phase: 1,
        total_phases,
        status: RolloutStatus::InProgress,
        phase_started_at: Utc::now(),
        phase_observation_end: None,
        systems_in_current_phase: first_phase_systems,
        systems_completed: vec![],
        systems_failed: vec![],
        halted_reason: None,
    })
}

/// Check if deployment can proceed based on rollout state
pub async fn check_rollout(
    pool: &PgPool,
    context: RolloutContext,
    context_id: &str,
    policy_id: Uuid,
    config: &CanaryConfig,
    all_systems: &[Uuid],
) -> Result<CanaryResult, sqlx::Error> {
    // Check if rollout exists
    let existing_state = get_rollout_state(pool, context, context_id, policy_id).await?;

    match existing_state {
        None => {
            // No rollout exists - initialize new one
            let state = init_rollout(pool, context, context_id, policy_id, config, all_systems).await?;
            Ok(CanaryResult {
                deployment_allowed: true,
                systems_to_deploy: state.systems_in_current_phase.clone(),
                reason: Some(format!("Starting canary rollout phase 1/{}", state.total_phases)),
                rollout_state: Some(state),
            })
        }
        Some(state) => {
            match state.status {
                RolloutStatus::Completed => {
                    Ok(CanaryResult {
                        deployment_allowed: true,
                        systems_to_deploy: all_systems.to_vec(),
                        reason: Some("Canary rollout completed".to_string()),
                        rollout_state: Some(state),
                    })
                }
                RolloutStatus::Halted | RolloutStatus::Failed => {
                    Ok(CanaryResult {
                        deployment_allowed: false,
                        systems_to_deploy: vec![],
                        reason: state.halted_reason.clone(),
                        rollout_state: Some(state),
                    })
                }
                RolloutStatus::Observing => {
                    // Check if observation period has ended
                    if let Some(end_time) = state.phase_observation_end {
                        if Utc::now() >= end_time {
                            // Observation period complete - advance to next phase
                            let next_state = advance_to_next_phase(pool, state.id, all_systems, config).await?;
                            Ok(CanaryResult {
                                deployment_allowed: true,
                                systems_to_deploy: next_state.systems_in_current_phase.clone(),
                                reason: Some(format!(
                                    "Observation complete, advanced to phase {}/{}",
                                    next_state.current_phase,
                                    next_state.total_phases
                                )),
                                rollout_state: Some(next_state),
                            })
                        } else {
                            // Still observing
                            Ok(CanaryResult {
                                deployment_allowed: false,
                                systems_to_deploy: vec![],
                                reason: Some(format!(
                                    "In observation period until {}",
                                    end_time.format("%Y-%m-%d %H:%M:%S UTC")
                                )),
                                rollout_state: Some(state),
                            })
                        }
                    } else {
                        // No observation end time set - shouldn't happen, but allow deployment
                        Ok(CanaryResult {
                            deployment_allowed: true,
                            systems_to_deploy: state.systems_in_current_phase.clone(),
                            reason: Some("Observation period misconfigured, allowing deployment".to_string()),
                            rollout_state: Some(state),
                        })
                    }
                }
                RolloutStatus::InProgress => {
                    // Currently deploying to phase systems
                    Ok(CanaryResult {
                        deployment_allowed: true,
                        systems_to_deploy: state.systems_in_current_phase.clone(),
                        reason: Some(format!(
                            "Deploying phase {}/{} ({} systems)",
                            state.current_phase,
                            state.total_phases,
                            state.systems_in_current_phase.len()
                        )),
                        rollout_state: Some(state),
                    })
                }
            }
        }
    }
}

/// Mark phase as complete and start observation period
pub async fn complete_phase(
    pool: &PgPool,
    rollout_id: Uuid,
    config: &CanaryConfig,
    successful_systems: &[Uuid],
    failed_systems: &[Uuid],
) -> Result<(), sqlx::Error> {
    let observation_end = Utc::now() + Duration::minutes(config.observe_duration_minutes as i64);

    // Check fail threshold
    if failed_systems.len() > config.health_check.fail_threshold as usize {
        // Halt rollout due to failures
        sqlx::query(
            r#"
            UPDATE canary_rollout_state
            SET status = 'halted',
                halted_reason = $2,
                updated_at = now()
            WHERE id = $1
            "#,
        )
        .bind(rollout_id)
        .bind(format!(
            "Health check failed: {}/{} systems failed (threshold: {})",
            failed_systems.len(),
            successful_systems.len() + failed_systems.len(),
            config.health_check.fail_threshold
        ))
        .execute(pool)
        .await?;
    } else {
        // Start observation period
        sqlx::query(
            r#"
            UPDATE canary_rollout_state
            SET status = 'observing',
                phase_observation_end = $2,
                systems_completed = array_cat(systems_completed, $3),
                systems_failed = array_cat(systems_failed, $4),
                updated_at = now()
            WHERE id = $1
            "#,
        )
        .bind(rollout_id)
        .bind(observation_end)
        .bind(successful_systems)
        .bind(failed_systems)
        .execute(pool)
        .await?;
    }

    Ok(())
}

/// Advance to next phase after observation period
pub async fn advance_to_next_phase(
    pool: &PgPool,
    rollout_id: Uuid,
    all_systems: &[Uuid],
    config: &CanaryConfig,
) -> Result<RolloutState, sqlx::Error> {
    let state = get_rollout_state_by_id(pool, rollout_id).await?
        .ok_or_else(|| sqlx::Error::RowNotFound)?;

    if state.current_phase >= state.total_phases {
        // Rollout complete
        sqlx::query(
            r#"
            UPDATE canary_rollout_state
            SET status = 'completed',
                completed_at = now(),
                updated_at = now()
            WHERE id = $1
            "#,
        )
        .bind(rollout_id)
        .execute(pool)
        .await?;

        return Ok(RolloutState {
            status: RolloutStatus::Completed,
            ..state
        });
    }

    let next_phase = state.current_phase + 1;
    let next_systems = select_systems_for_phase(
        all_systems,
        &state.systems_completed,
        &state.systems_failed,
        config.percentage,
        &config.selection_strategy,
        next_phase,
    );

    sqlx::query(
        r#"
        UPDATE canary_rollout_state
        SET current_phase = $2,
            status = 'in_progress',
            systems_in_current_phase = $3,
            phase_started_at = now(),
            phase_observation_end = NULL,
            updated_at = now()
        WHERE id = $1
        "#,
    )
    .bind(rollout_id)
    .bind(next_phase)
    .bind(&next_systems)
    .execute(pool)
    .await?;

    Ok(RolloutState {
        current_phase: next_phase,
        status: RolloutStatus::InProgress,
        systems_in_current_phase: next_systems,
        phase_started_at: Utc::now(),
        phase_observation_end: None,
        ..state
    })
}

/// Get rollout state for a deployment context
pub async fn get_rollout_state(
    pool: &PgPool,
    context: RolloutContext,
    context_id: &str,
    policy_id: Uuid,
) -> Result<Option<RolloutState>, sqlx::Error> {
    let result = sqlx::query_as::<_, (
        Uuid,
        i32,
        i32,
        String,
        DateTime<Utc>,
        Option<DateTime<Utc>>,
        Vec<Uuid>,
        Vec<Uuid>,
        Vec<Uuid>,
        Option<String>,
    )>(
        r#"
        SELECT id, current_phase, total_phases, status, phase_started_at,
               phase_observation_end, systems_in_current_phase, systems_completed,
               systems_failed, halted_reason
        FROM canary_rollout_state
        WHERE rollout_context_type = $1
          AND rollout_context_id = $2
          AND policy_id = $3
        ORDER BY created_at DESC
        LIMIT 1
        "#,
    )
    .bind(context.as_str())
    .bind(context_id)
    .bind(policy_id)
    .fetch_optional(pool)
    .await?;

    Ok(result.map(|row| RolloutState {
        id: row.0,
        current_phase: row.1,
        total_phases: row.2,
        status: RolloutStatus::from_str(&row.3),
        phase_started_at: row.4,
        phase_observation_end: row.5,
        systems_in_current_phase: row.6,
        systems_completed: row.7,
        systems_failed: row.8,
        halted_reason: row.9,
    }))
}

/// Get rollout state by ID
async fn get_rollout_state_by_id(
    pool: &PgPool,
    rollout_id: Uuid,
) -> Result<Option<RolloutState>, sqlx::Error> {
    let result = sqlx::query_as::<_, (
        Uuid,
        i32,
        i32,
        String,
        DateTime<Utc>,
        Option<DateTime<Utc>>,
        Vec<Uuid>,
        Vec<Uuid>,
        Vec<Uuid>,
        Option<String>,
    )>(
        r#"
        SELECT id, current_phase, total_phases, status, phase_started_at,
               phase_observation_end, systems_in_current_phase, systems_completed,
               systems_failed, halted_reason
        FROM canary_rollout_state
        WHERE id = $1
        "#,
    )
    .bind(rollout_id)
    .fetch_optional(pool)
    .await?;

    Ok(result.map(|row| RolloutState {
        id: row.0,
        current_phase: row.1,
        total_phases: row.2,
        status: RolloutStatus::from_str(&row.3),
        phase_started_at: row.4,
        phase_observation_end: row.5,
        systems_in_current_phase: row.6,
        systems_completed: row.7,
        systems_failed: row.8,
        halted_reason: row.9,
    }))
}

/// Calculate total phases needed based on percentage
fn calculate_total_phases(percentage: u32) -> i32 {
    if percentage == 0 || percentage > 100 {
        return 1;
    }
    ((100.0 / percentage as f64).ceil()) as i32
}

/// Select systems for a given phase
fn select_systems_for_phase(
    all_systems: &[Uuid],
    completed: &[Uuid],
    failed: &[Uuid],
    percentage: u32,
    strategy: &str,
    phase: i32,
) -> Vec<Uuid> {
    let completed_set: HashSet<_> = completed.iter().collect();
    let failed_set: HashSet<_> = failed.iter().collect();
    
    let remaining: Vec<Uuid> = all_systems
        .iter()
        .filter(|s| !completed_set.contains(s) && !failed_set.contains(s))
        .cloned()
        .collect();

    if remaining.is_empty() {
        return vec![];
    }

    let phase_size = ((all_systems.len() as f64 * percentage as f64) / 100.0).ceil() as usize;
    let phase_size = phase_size.min(remaining.len());

    match strategy {
        "random" => {
            use rand::seq::SliceRandom;
            let mut rng = rand::thread_rng();
            let mut selected = remaining.clone();
            selected.shuffle(&mut rng);
            selected.into_iter().take(phase_size).collect()
        }
        "hash-based" => {
            // Deterministic selection based on system UUID hash + phase
            let mut indexed: Vec<(u64, Uuid)> = remaining
                .iter()
                .map(|uuid| {
                    let hash = uuid.as_u128() as u64 ^ (phase as u64);
                    (hash, *uuid)
                })
                .collect();
            indexed.sort_by_key(|(hash, _)| *hash);
            indexed.into_iter().take(phase_size).map(|(_, uuid)| uuid).collect()
        }
        _ => {
            // Default: simple sequential selection
            remaining.into_iter().take(phase_size).collect()
        }
    }
}
