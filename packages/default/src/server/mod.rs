use crate::config::{CrystalForgeConfig, FlakeConfig};
use crate::deployment::spawn_deployment_policy_manager;
use crate::flake::commits::sync_all_watched_flakes_commits_with_ids;
use crate::log::log_builder_worker_status;
use crate::models::commits::Commit;
use crate::models::deployment_policies::DeploymentPolicy;
use crate::models::evaluate_with_policies::{
    evaluate_with_mock_eval_jobs, evaluate_with_nix_eval_jobs, update_commit_metadata_cache,
};
use crate::models::flakes::Flake;
use crate::queue::QueueNotifier;
// NOTE: removed increment_commit_list_attempt_count – we now rely on the new evaluation_* fields
use crate::queries::flakes::get_all_flakes_from_db_with_ids;
use anyhow::Result;
use sqlx::PgPool;
use std::sync::Arc;
use tokio::time;
use tokio::time::Duration;
use tokio::time::Instant;
use tokio::time::interval;
use tracing::{debug, error, info, warn};

// ⬇️ bring in the commit-eval helpers you said you added in queries/commits.rs
use crate::queries::build_jobs::create_build_jobs_for_commit;
use crate::queries::builders::cleanup_expired_build_logs;
use crate::queries::commits::{
    get_commits_pending_evaluation, mark_commit_evaluation_complete, mark_commit_evaluation_failed,
    mark_commit_evaluation_started, reset_stuck_commit_evaluations,
};
use crate::queries::deployment_policies::list_enabled_deployment_policies;
use crate::queries::derivations::{cleanup_partial_derivations, reset_stuck_builds};

fn custom_field_name(name: &str, id: uuid::Uuid) -> String {
    let mut slug = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>();
    while slug.contains("__") {
        slug = slug.replace("__", "_");
    }
    let slug = slug.trim_matches('_');
    let short_id = id.to_string();
    let short_id = &short_id[..8.min(short_id.len())];
    if slug.is_empty() {
        format!("custom_{}", short_id)
    } else {
        format!("{}_{}", slug, short_id)
    }
}

fn normalize_custom_policy_expression(expression: &str) -> (String, bool) {
    let mut cursor = 0usize;
    let mut changed = false;
    let mut normalized = String::with_capacity(expression.len() + 16);

    while let Some(rel_idx) = expression[cursor..].find("config.") {
        let idx = cursor + rel_idx;
        let prev_char = expression[..idx].chars().next_back();
        let has_safe_boundary = prev_char
            .map(|c| !(c.is_ascii_alphanumeric() || c == '_' || c == '.'))
            .unwrap_or(true);
        let already_cfg_prefixed = idx >= 4 && expression.get(idx - 4..idx) == Some("cfg.");

        normalized.push_str(&expression[cursor..idx]);

        if has_safe_boundary && !already_cfg_prefixed {
            normalized.push_str("cfg.config.");
            changed = true;
        } else {
            normalized.push_str("config.");
        }

        cursor = idx + "config.".len();
    }

    normalized.push_str(&expression[cursor..]);
    (normalized, changed)
}

fn parse_deployment_policy_record(
    record: &crate::models::deployment_policies::DeploymentPolicyRecord,
) -> Option<DeploymentPolicy> {
    let cfg = &record.config;
    match record.policy_type.as_str() {
        "require_cf_agent" => {
            if cfg.get("strict").and_then(|v| v.as_bool()) == Some(false) {
                warn!(
                    "Ignoring strict=false for require_cf_agent policy '{}' ({}); enforcing strict=true",
                    record.name, record.id
                );
            }
            Some(DeploymentPolicy::RequireCrystalForgeAgent { strict: true })
        }
        "require_packages" => {
            let strict = cfg.get("strict").and_then(|v| v.as_bool()).unwrap_or(true);
            let packages = cfg
                .get("packages")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            Some(DeploymentPolicy::RequirePackages { packages, strict })
        }
        "custom_check" => {
            let expression = cfg
                .get("expression")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let Some(expression) = expression else {
                warn!(
                    "Skipping custom_check policy '{}' ({}): missing config.expression",
                    record.name, record.id
                );
                return None;
            };
            let (expression, normalized_legacy_ref) =
                normalize_custom_policy_expression(&expression);
            if normalized_legacy_ref {
                warn!(
                    "Auto-normalized legacy custom_check expression for policy '{}' ({}): replaced `config.` with `cfg.config.`",
                    record.name, record.id
                );
            }

            let description = cfg
                .get("description")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .or_else(|| record.description.clone())
                .unwrap_or_else(|| format!("Custom policy: {}", record.name));

            let field_name = cfg
                .get("field_name")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| custom_field_name(&record.name, record.id));

            let strict = cfg.get("strict").and_then(|v| v.as_bool()).unwrap_or(false);

            Some(DeploymentPolicy::CustomCheck {
                expression,
                description,
                field_name,
                strict,
            })
        }
        other => {
            warn!(
                "Skipping unsupported deployment policy type '{}' for policy '{}' ({})",
                other, record.name, record.id
            );
            None
        }
    }
}

async fn load_deployment_policies_for_eval(pool: &PgPool) -> Vec<DeploymentPolicy> {
    match list_enabled_deployment_policies(pool).await {
        Ok(records) => {
            let mut policies = records
                .iter()
                .filter_map(parse_deployment_policy_record)
                .collect::<Vec<_>>();

            if policies.is_empty() {
                warn!(
                    "No valid deployment policies found in DB, falling back to strict CF agent check"
                );
                // Use strict mode in fallback to enforce core security policy even in error scenarios.
                // This ensures systems without the agent package cannot pass evaluation when policy
                // loading fails, maintaining the "always enforce core policy" safety model.
                policies.push(DeploymentPolicy::RequireCrystalForgeAgent { strict: true });
            }

            policies
        }
        Err(err) => {
            error!(
                "Failed to load deployment policies from DB for evaluation: {:#}. Falling back to strict CF agent check",
                err
            );
            // Use strict mode in fallback to enforce core security policy even in error scenarios.
            // This ensures systems without the agent package cannot pass evaluation when policy
            // loading fails, maintaining the "always enforce core policy" safety model.
            vec![DeploymentPolicy::RequireCrystalForgeAgent { strict: true }]
        }
    }
}

pub fn spawn_background_tasks(
    cfg: CrystalForgeConfig,
    pool: PgPool,
    cf_state: Arc<crate::handlers::agent_request::CFState>,
    queue_notifier: Arc<QueueNotifier>,
) {
    let flake_pool = pool.clone();
    let commit_pool = pool.clone();
    let target_pool = pool.clone();
    let deployment_pool = pool.clone();
    let artifact_pool = pool.clone();
    let build_log_pool = pool.clone();

    // Get the flake config with a fallback
    let flake_config = cfg.flakes.clone();

    tokio::spawn(run_flake_polling_loop(
        flake_pool,
        flake_config.clone(),
        queue_notifier.clone(),
    ));
    tokio::spawn(run_commit_evaluation_loop(
        commit_pool,
        flake_config.commit_evaluation_interval,
        cf_state,
        queue_notifier.clone(),
    ));
    tokio::spawn(run_commit_artifact_hydration_loop(artifact_pool));
    tokio::spawn(run_build_log_retention_loop(
        build_log_pool,
        cfg.server.build_log_retention_days,
        cfg.server.failed_build_log_retention_days,
    ));

    let commit_cache_pool = pool.clone();
    tokio::spawn(run_commit_cache_gc_loop(
        commit_cache_pool,
        cfg.server.commit_cache_retention_days,
    ));

    tokio::spawn(spawn_deployment_policy_manager(cfg, deployment_pool));
}

/// Runs daily build log retention cleanup.
///
/// Clears old logs to prevent unbounded growth in build_jobs.logs.
async fn run_build_log_retention_loop(
    pool: PgPool,
    success_retention_days: i32,
    failed_retention_days: i32,
) {
    info!(
        "🔁 Starting build log retention loop (success={}d, failed={}d)",
        success_retention_days, failed_retention_days
    );

    let mut ticker = interval(Duration::from_secs(24 * 60 * 60));

    loop {
        match cleanup_expired_build_logs(&pool, success_retention_days, failed_retention_days).await
        {
            Ok((success_cleared, failed_cleared)) => {
                if success_cleared > 0 || failed_cleared > 0 {
                    info!(
                        "🧹 Cleared expired build logs: success={}, failed={}",
                        success_cleared, failed_cleared
                    );
                } else {
                    debug!("Build log retention: no expired logs to clear");
                }
            }
            Err(err) => {
                error!("❌ Build log retention cleanup failed: {:#}", err);
            }
        }

        ticker.tick().await;
    }
}

/// Runs daily commit metadata cache garbage collection.
///
/// Removes cache entries older than retention period to prevent unbounded growth.
async fn run_commit_cache_gc_loop(pool: PgPool, retention_days: i32) {
    let retention_days = if retention_days <= 0 {
        warn!(
            "Invalid commit cache retention_days={} (must be > 0); defaulting to 30 days",
            retention_days
        );
        30
    } else {
        retention_days
    };

    info!(
        "🔁 Starting commit metadata cache GC loop (retention={}d)",
        retention_days
    );

    let mut ticker = interval(Duration::from_secs(24 * 60 * 60));

    loop {
        ticker.tick().await;

        match crate::tasks::gc_commit_cache::garbage_collect_commit_cache(&pool, retention_days)
            .await
        {
            Ok(deleted) => {
                if deleted > 0 {
                    debug!("Commit cache GC completed: {} entries removed", deleted);
                }
            }
            Err(err) => {
                error!("❌ Commit cache GC failed: {:#}", err);
            }
        }
    }
}

/// Runs the periodic flake polling loop to check for new commits
async fn run_flake_polling_loop(
    pool: PgPool,
    flake_config: FlakeConfig,
    queue_notifier: Arc<QueueNotifier>,
) {
    info!("🔄 Starting periodic flake polling loop...");
    loop {
        // Get all flakes from database instead of just config ones (with their IDs for credential loading)
        match get_all_flakes_from_db_with_ids(&pool, &flake_config).await {
            Ok((db_flakes, flake_ids)) => {
                if !db_flakes.is_empty() {
                    match sync_all_watched_flakes_commits_with_ids(&pool, &db_flakes, &flake_ids).await {
                        Ok(total_inserted) => {
                            if total_inserted > 0 {
                                info!(
                                    "📥 Inserted {} new commits, notifying eval queue",
                                    total_inserted
                                );
                                queue_notifier.notify_eval_queue();
                            }
                        }
                        Err(e) => error!("❌ Error in flake polling cycle: {e}"),
                    }
                }
            }
            Err(e) => error!("❌ Failed to get flakes from database: {e}"),
        }
        tokio::time::sleep(flake_config.flake_polling_interval).await;
    }
}

/// Runs the event-driven commit evaluation loop with fallback polling.
///
/// Uses `tokio::select!` to listen for:
/// 1. Queue notifications (immediate processing when commits arrive)
/// 2. Periodic ticker (fallback to catch any missed notifications)
pub async fn run_commit_evaluation_loop(
    pool: PgPool,
    interval: Duration,
    cf_state: Arc<crate::handlers::agent_request::CFState>,
    queue_notifier: Arc<QueueNotifier>,
) {
    info!(
        "🔁 Starting event-driven commit evaluation loop (fallback every {:?})...",
        interval
    );

    // ⬇️ cleanup any stranded 'in_progress' from previous runs
    if let Err(e) = reset_stuck_commit_evaluations(&pool).await {
        error!("❌ Failed to reset stuck commit evaluations: {}", e);
    }

    if let Err(e) = reset_stuck_builds(&pool).await {
        error!("❌ Failed to reset stuck builds: {}", e);
    }

    if let Err(e) = cleanup_partial_derivations(&pool).await {
        error!("❌ Failed to reset partial derivations: {}", e);
    }

    // `PgPool` is cheap to clone; keep an owned copy in the task.
    let pool = pool.clone();

    // Use an interval ticker as fallback to catch missed notifications
    let mut ticker = time::interval_at(Instant::now() + interval, interval);

    loop {
        // ALWAYS check for pending work first (in case notification was sent before we started waiting)
        if let Err(e) = process_pending_commits(&pool, &cf_state, &queue_notifier).await {
            error!("❌ Error in commit evaluation cycle: {e}");
        }

        // Wait for either a notification or the periodic ticker before checking again
        tokio::select! {
            _ = ticker.tick() => {
                debug!("⏰ Eval loop: periodic tick (fallback polling)");
            }
            _ = queue_notifier.wait_for_eval_work() => {
                debug!("🔔 Eval loop: notified of new work");
            }
        }
    }
}

async fn process_pending_commits(
    pool: &PgPool,
    cf_state: &Arc<crate::handlers::agent_request::CFState>,
    queue_notifier: &Arc<QueueNotifier>,
) -> Result<()> {
    loop {
        let pending_commits = match get_commits_pending_evaluation(pool).await {
            Ok(commits) => commits,
            Err(e) => {
                error!("❌ Failed to get pending commits: {e}");
                return Ok(());
            }
        };

        if pending_commits.is_empty() {
            return Ok(());
        }

        info!("📌 Found {} pending commits", pending_commits.len());
        let Some(next_commit_id) =
            select_next_pending_commit_id_for_cycle(pending_commits.iter().map(|c| c.id))
        else {
            return Ok(());
        };

        let Some(commit) = pending_commits.into_iter().find(|c| c.id == next_commit_id) else {
            return Ok(());
        };
        // ⬇️ mark STARTED (bumps evaluation_attempt_count internally)
        if let Err(e) = mark_commit_evaluation_started(pool, commit.id).await {
            let error_text = e.to_string();
            if error_text.contains("another commit is already being evaluated") {
                debug!(
                    "⏭️ Eval start race for commit {} ({}): another worker/loop iteration already claimed in_progress",
                    commit.id, commit.git_commit_hash
                );
                return Ok(());
            } else {
                error!(
                    "❌ Could not mark commit {} evaluation started: {}",
                    commit.git_commit_hash, e
                );
            }
            return Ok(());
        }

        // Get flake info (post-claim; failures now go through retry/defer path)
        let flake = match commit.get_flake(&pool).await {
            Ok(flake) => flake,
            Err(e) => {
                error!(
                    "❌ Failed to get flake for commit {}: {}",
                    commit.git_commit_hash, e
                );
                let _ = mark_commit_evaluation_failed(pool, commit.id, &e.to_string()).await;
                return Ok(());
            }
        };

        // Load Crystal Forge config to get build settings (post-claim retry/defer path)
        let cfg = match CrystalForgeConfig::load() {
            Ok(cfg) => cfg,
            Err(e) => {
                error!("❌ Failed to load config: {}", e);
                let _ = mark_commit_evaluation_failed(pool, commit.id, &e.to_string()).await;
                return Ok(());
            }
        };
        let build_config = cfg.get_build_config();
        let server_config = cfg.get_server_config();
        let mock_systems = cfg
            .systems
            .iter()
            .filter(|s| s.flake_name.as_deref() == Some(flake.name.as_str()))
            .map(|s| s.hostname.clone())
            .collect::<Vec<_>>();

        // Load enabled deployment policies from DB for nix-eval-jobs policy checks.
        let policies = load_deployment_policies_for_eval(pool).await;

        // CRITICAL: Create broadcast channel BEFORE eval starts
        // This ensures WebSocket clients can subscribe before messages are sent
        crate::handlers::api::commits::ensure_eval_channel(&cf_state, commit.id).await;

        // Broadcast eval start status to WebSocket clients
        crate::handlers::api::commits::broadcast_eval_status(
            &cf_state,
            commit.id,
            "started".to_string(),
            Some(format!(
                "Starting evaluation for commit {}",
                &commit.git_commit_hash[..7.min(commit.git_commit_hash.len())]
            )),
        )
        .await;
        crate::handlers::api::commits::broadcast_eval_log(
            &cf_state,
            commit.id,
            format!(
                "🚀 Starting evaluation for commit {}",
                commit.git_commit_hash
            ),
        )
        .await;

        // Use nix-eval-jobs to discover AND evaluate all nixosConfigurations
        // This will:
        // 1. Evaluate all systems in parallel
        // 2. Check deployment policies (CF agent status) for each system
        // 3. Store policy results in database (cf_agent_enabled column)
        // 4. Insert/update derivation records
        let eval_result = if server_config.execution_mode.is_mock() {
            info!(
                "🧪 Using MOCK evaluation mode for commit {}",
                commit.git_commit_hash
            );
            evaluate_with_mock_eval_jobs(
                pool,
                &commit,
                &flake,
                &flake.repo_url,
                &commit.git_commit_hash,
                "all",
                &build_config,
                &server_config,
                &policies,
                &mock_systems,
                Some(&cf_state),
                Some(&queue_notifier),
            )
            .await
        } else {
            evaluate_with_nix_eval_jobs(
                pool,
                &commit,
                &flake,
                &flake.repo_url,
                &commit.git_commit_hash,
                "all", // Evaluate all systems
                &build_config,
                &server_config,
                &policies,
                Some(&cf_state), // Pass CFState for WebSocket broadcasting
                Some(&queue_notifier),
            )
            .await
        };

        match eval_result {
            Ok((results, policy_checks)) => {
                // Broadcast completion status
                crate::handlers::api::commits::broadcast_eval_status(
                    &cf_state,
                    commit.id,
                    "complete".to_string(),
                    Some(format!("Evaluated {} systems", results.len())),
                )
                .await;
                crate::handlers::api::commits::broadcast_eval_log(
                    &cf_state,
                    commit.id,
                    format!(
                        "✅ Evaluation complete for commit {}",
                        commit.git_commit_hash
                    ),
                )
                .await;

                // Cleanup WebSocket broadcast channel
                crate::handlers::api::commits::cleanup_eval_channel(&cf_state, commit.id).await;

                // ⬇️ mark COMPLETE
                if let Err(e) = mark_commit_evaluation_complete(pool, commit.id).await {
                    error!(
                        "❌ Failed to mark commit {} evaluation complete: {}",
                        commit.git_commit_hash, e
                    );
                }

                // ⬇️ UPDATE CACHE with evaluation summary
                if let Err(e) =
                    update_commit_metadata_cache(pool, commit.id, &policy_checks, false).await
                {
                    error!(
                        "❌ Failed to update commit metadata cache for {}: {}",
                        commit.git_commit_hash, e
                    );
                }

                // ⬇️ CREATE BUILD JOBS for evaluated derivations
                match create_build_jobs_for_commit(pool, commit.id).await {
                    Ok(job_count) if job_count > 0 => {
                        info!(
                            "📋 Queued {} build jobs for commit {}, notifying build workers",
                            job_count, commit.git_commit_hash
                        );
                        // Notify build queue that new work is available
                        queue_notifier.notify_build_queue();
                    }
                    Ok(_) => {
                        debug!(
                            "No new build jobs for commit {} (already queued or no ready derivations)",
                            commit.git_commit_hash
                        );
                    }
                    Err(e) => {
                        error!(
                            "❌ Failed to create build jobs for commit {}: {}",
                            commit.git_commit_hash, e
                        );
                        // Don't fail the whole evaluation if job creation fails
                    }
                }

                let total = results.len();
                let with_agent = policy_checks
                    .iter()
                    .filter(|check| check.cf_agent_enabled == Some(true))
                    .count();

                info!(
                    "✅ Evaluated {} NixOS configurations for commit {}",
                    total, commit.git_commit_hash
                );
                info!(
                    "   CF agent: {}/{} systems enabled ({:.1}%)",
                    with_agent,
                    policy_checks.len(),
                    if policy_checks.len() > 0 {
                        (with_agent as f64 / policy_checks.len() as f64) * 100.0
                    } else {
                        0.0
                    }
                );

                // Log any policy warnings
                for check in policy_checks.iter().filter(|c| !c.meets_requirements) {
                    for warning in &check.warnings {
                        warn!("⚠️  {}: {}", check.system_name, warning);
                    }
                }
            }
            Err(e) => {
                error!(
                    "❌ Failed to evaluate commit {}: {}",
                    commit.git_commit_hash, e
                );

                // Broadcast failure status
                crate::handlers::api::commits::broadcast_eval_status(
                    &cf_state,
                    commit.id,
                    "failed".to_string(),
                    Some(format!("Evaluation failed: {}", e)),
                )
                .await;
                crate::handlers::api::commits::broadcast_eval_log(
                    &cf_state,
                    commit.id,
                    format!("❌ Evaluation failed: {}", e),
                )
                .await;

                // Cleanup WebSocket broadcast channel
                crate::handlers::api::commits::cleanup_eval_channel(&cf_state, commit.id).await;

                // ⬇️ mark FAILED (function will set 'pending' or terminal 'failed'
                // depending on attempt limit inside your SQL)
                if let Err(mark_err) =
                    mark_commit_evaluation_failed(pool, commit.id, &e.to_string()).await
                {
                    error!(
                        "❌ Failed to mark commit {} evaluation failed: {}",
                        commit.git_commit_hash, mark_err
                    );
                }

                // ⬇️ UPDATE CACHE to record eval error (no policy checks available)
                if let Err(cache_err) =
                    update_commit_metadata_cache(pool, commit.id, &[], true).await
                {
                    error!(
                        "❌ Failed to update commit metadata cache for {}: {}",
                        commit.git_commit_hash, cache_err
                    );
                }

                return Ok(());
            }
        }
    }
}

fn select_next_pending_commit_id_for_cycle(
    mut pending_commit_ids: impl Iterator<Item = i32>,
) -> Option<i32> {
    pending_commit_ids.next()
}

#[cfg(test)]
mod tests {
    use super::{
        normalize_custom_policy_expression, parse_deployment_policy_record,
        select_next_pending_commit_id_for_cycle,
    };
    use crate::models::deployment_policies::{DeploymentPolicy, DeploymentPolicyRecord};
    use chrono::Utc;
    use serde_json::json;
    use uuid::Uuid;

    #[test]
    fn select_next_pending_commit_id_honors_latest_reordered_snapshot() {
        let first_cycle = vec![10, 20, 30];
        assert_eq!(
            select_next_pending_commit_id_for_cycle(first_cycle.into_iter()),
            Some(10)
        );

        // Simulate DB reorder before next cycle re-query.
        let reordered_cycle = vec![30, 20];
        assert_eq!(
            select_next_pending_commit_id_for_cycle(reordered_cycle.into_iter()),
            Some(30)
        );
    }

    #[test]
    fn select_next_pending_commit_id_allows_progress_when_prior_head_is_deferred() {
        // Simulate prior head being deferred by failure handling before next cycle.
        let next_cycle = vec![22, 23, 24];
        assert_eq!(
            select_next_pending_commit_id_for_cycle(next_cycle.into_iter()),
            Some(22)
        );
    }

    #[test]
    fn parse_require_cf_agent_enforces_strict_true() {
        let record = DeploymentPolicyRecord {
            id: Uuid::new_v4(),
            name: "core".to_string(),
            description: Some("core policy".to_string()),
            policy_type: "require_cf_agent".to_string(),
            config: json!({"strict": false}),
            enabled: true,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        let parsed = parse_deployment_policy_record(&record).expect("policy should parse");
        match parsed {
            DeploymentPolicy::RequireCrystalForgeAgent { strict } => assert!(strict),
            _ => panic!("expected RequireCrystalForgeAgent variant"),
        }
    }

    #[test]
    fn normalize_custom_policy_expression_rewrites_legacy_config_prefix() {
        let (normalized, changed) =
            normalize_custom_policy_expression("config.services.auditd.enable or false");
        assert!(changed);
        assert_eq!(normalized, "cfg.config.services.auditd.enable or false");
    }

    #[test]
    fn normalize_custom_policy_expression_keeps_cfg_config_prefix() {
        let (normalized, changed) =
            normalize_custom_policy_expression("cfg.config.networking.firewall.enable");
        assert!(!changed);
        assert_eq!(normalized, "cfg.config.networking.firewall.enable");
    }

    #[test]
    fn parse_custom_check_normalizes_expression() {
        let record = DeploymentPolicyRecord {
            id: Uuid::new_v4(),
            name: "auditd".to_string(),
            description: Some("auditd enabled".to_string()),
            policy_type: "custom_check".to_string(),
            config: json!({
                "expression": "config.services.auditd.enable or false",
                "strict": false
            }),
            enabled: true,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        let parsed = parse_deployment_policy_record(&record).expect("policy should parse");
        match parsed {
            DeploymentPolicy::CustomCheck { expression, .. } => {
                assert_eq!(expression, "cfg.config.services.auditd.enable or false")
            }
            _ => panic!("expected CustomCheck variant"),
        }
    }
}

/// Background task to hydrate commit artifact cache (nixosConfigurations + changed files).
/// Processes commits with missing cache entries, with progressive backoff on failure.
async fn run_commit_artifact_hydration_loop(pool: PgPool) {
    use crate::flake::commits::{get_commit_changed_files, get_commit_nixos_configurations};
    use crate::queries::commits_artifacts::{
        get_commits_needing_artifact_cache, mark_commit_artifact_hydration_failed,
        upsert_commit_artifact_cache,
    };

    info!("🔁 Starting commit artifact hydration background task...");

    let pool = pool.clone();
    let mut ticker = interval(Duration::from_secs(30)); // Check every 30 seconds

    loop {
        ticker.tick().await;

        // Process up to 3 commits per cycle (sequential to avoid overwhelming nix eval)
        match get_commits_needing_artifact_cache(&pool, 3).await {
            Ok(commits) if !commits.is_empty() => {
                for (commit_id, commit_hash, repo_url) in commits {
                    info!(
                        "🔍 Hydrating commit artifacts for {} @ {}",
                        repo_url, commit_hash
                    );

                    // Try to get nixosConfigurations
                    let configs = match get_commit_nixos_configurations(
                        &repo_url,
                        &[commit_hash.clone()],
                    )
                    .await
                    .remove(&commit_hash)
                    {
                        Some(configs) => configs,
                        None => {
                            warn!(
                                "⚠️  Failed to get nixosConfigurations for {} @ {}, marking as failed",
                                repo_url, commit_hash
                            );
                            let _ = mark_commit_artifact_hydration_failed(&pool, commit_id).await;
                            continue;
                        }
                    };

                    // Try to get changed files (best effort)
                    let changed_files = get_commit_changed_files(&repo_url, &[commit_hash.clone()])
                        .await
                        .ok()
                        .and_then(|mut map| map.remove(&commit_hash))
                        .unwrap_or_default();

                    // Persist to cache
                    match upsert_commit_artifact_cache(&pool, commit_id, &configs, &changed_files)
                        .await
                    {
                        Ok(_) => {
                            info!(
                                "✅ Cached {} configs, {} files for {} @ {}",
                                configs.len(),
                                changed_files.len(),
                                repo_url,
                                commit_hash
                            );
                        }
                        Err(err) => {
                            error!(
                                "❌ Failed to persist cache for {} @ {}: {:#}",
                                repo_url, commit_hash, err
                            );
                        }
                    }
                }
            }
            Ok(_) => {
                debug!("No commits need artifact hydration");
            }
            Err(err) => {
                error!(
                    "❌ Failed to query commits needing artifact cache: {:#}",
                    err
                );
            }
        }
    }
}

pub async fn memory_monitor_task(pool: PgPool) {
    let mut interval = interval(Duration::from_secs(30));
    loop {
        interval.tick().await;
        log_memory_usage(&pool).await;
    }
}

async fn log_memory_usage(pool: &PgPool) {
    // Memory stats from /proc/self/status
    if let Ok(contents) = tokio::fs::read_to_string("/proc/self/status").await {
        let mut vm_rss = None;
        let mut vm_size = None;
        let mut vm_peak = None;

        for line in contents.lines() {
            if line.starts_with("VmRSS:") {
                vm_rss = line.split_whitespace().nth(1);
            } else if line.starts_with("VmSize:") {
                vm_size = line.split_whitespace().nth(1);
            } else if line.starts_with("VmPeak:") {
                vm_peak = line.split_whitespace().nth(1);
            }
        }

        debug!(
            "📊 Memory - RSS: {} kB, Size: {} kB, Peak: {} kB",
            vm_rss.unwrap_or("?"),
            vm_size.unwrap_or("?"),
            vm_peak.unwrap_or("?")
        );
    }

    // Database pool statistics
    let pool_size = pool.size() as usize;
    let idle_count = pool.num_idle();

    debug!(
        "📊 DB Pool - Total: {}, Idle: {}, Active: {}",
        pool_size,
        idle_count,
        pool_size - idle_count
    );

    log_builder_worker_status().await;
    // Task/thread count
    if let Ok(contents) = tokio::fs::read_to_string("/proc/self/stat").await {
        if let Some(num_threads) = contents.split_whitespace().nth(19) {
            debug!("📊 Threads: {}", num_threads);
        }
    }
}
