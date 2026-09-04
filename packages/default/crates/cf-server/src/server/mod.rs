pub mod jobs;

use crate::builder::run_cve_scan_loop;
use crate::compliance::canonical::semantic_digest;
use crate::compliance::resolver::{
    AssignmentMode, ResolutionOutcome, resolve_systems_effective_policies_for_evaluation_batch,
};
use crate::config::{CrystalForgeConfig, FlakeConfig};
use crate::deployment::spawn_deployment_policy_manager;
use crate::flake::commits::sync_all_watched_flakes_commits_with_ids;
use crate::log::log_builder_worker_status;
use crate::models::commits::Commit;
use crate::models::deployment_policies::DeploymentPolicy;
use crate::models::evaluate_with_policies::{
    EvaluationFinalizeOutcome, FinalizedDerivation, evaluate_with_mock_eval_jobs,
    evaluate_with_nix_eval_jobs, finalize_evaluation_attempt, update_commit_metadata_cache,
};
use crate::models::flakes::Flake;
use crate::queue::QueueNotifier;
use crate::server::jobs::{BackgroundJobHandle, BackgroundJobRegistry};
// NOTE: removed increment_commit_list_attempt_count – we now rely on the new evaluation_* fields
use crate::queries::flakes::get_all_flakes_from_db_with_ids;
use anyhow::{Context, Result};
use sqlx::PgPool;
use std::sync::{Arc, OnceLock};
use tokio::sync::Semaphore;
use tokio::time;
use tokio::time::Duration;
use tokio::time::Instant;
use tokio::time::interval;
use tracing::{debug, error, info, warn};

// ⬇️ bring in the commit-eval helpers you said you added in queries/commits.rs
use crate::derivations::utils::count_closure_packages;
use crate::models::deployment_policies::{AssignedPolicy, PoliciesByConfiguration};
use crate::queries::build_jobs::{QueuedBuild, recover_orphaned_derivation_build_jobs};
use crate::queries::builders::{
    cleanup_expired_build_logs, mark_stale_builders_offline,
    requeue_orphaned_building_jobs_with_reason,
};
use crate::queries::commits::{
    EvalCancellationOutcome, EvalFailureOutcome, EvalStartOutcome, get_commits_pending_evaluation,
    mark_commit_evaluation_failed, mark_commit_evaluation_started, next_evaluation_available_at,
    reset_stuck_commit_evaluations,
};
use crate::queries::deployment_policies::{
    get_deployment_policies_by_versions, list_enabled_deployment_policies,
    list_enabled_policies_for_flake, list_policy_rows_by_configuration_for_flake,
};
use crate::queries::derivations::{
    cleanup_partial_derivations, reset_stuck_builds, set_closure_counts,
};
use crate::services::hardening_scans::trigger_commit_hardening_scans;

const CLOSURE_COUNT_MAX_CONCURRENT: usize = 2;
static CLOSURE_COUNT_LIMITER: OnceLock<Arc<Semaphore>> = OnceLock::new();

/// Maximum number of commit evaluations (nix-eval-jobs + fallback phase)
/// that may run concurrently across the entire server process.
/// Each bulk evaluation loads and evaluates a large flake and can use
/// several GiB of memory; running more than one at a time on this host
/// risks memory exhaustion when combined with the standalone fallback evals.
const MAX_CONCURRENT_COMMIT_EVALUATIONS: usize = 1;
static COMMIT_EVALUATION_LIMITER: OnceLock<Arc<Semaphore>> = OnceLock::new();

fn commit_evaluation_limiter() -> Arc<Semaphore> {
    COMMIT_EVALUATION_LIMITER
        .get_or_init(|| Arc::new(Semaphore::new(MAX_CONCURRENT_COMMIT_EVALUATIONS)))
        .clone()
}

fn closure_count_limiter() -> Arc<Semaphore> {
    CLOSURE_COUNT_LIMITER
        .get_or_init(|| Arc::new(Semaphore::new(CLOSURE_COUNT_MAX_CONCURRENT)))
        .clone()
}

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

pub(crate) fn normalize_custom_policy_expression(expression: &str) -> (String, bool) {
    let chars = expression.chars().collect::<Vec<_>>();
    let legacy = "cfg.config.";
    let legacy_chars = legacy.chars().collect::<Vec<_>>();
    let mut output = String::with_capacity(expression.len());
    let mut index = 0;
    let mut state = LexicalState::Normal;

    while index < chars.len() {
        match state {
            LexicalState::Normal => {
                if chars[index] == '"' {
                    state = LexicalState::DoubleQuoted;
                    output.push(chars[index]);
                    index += 1;
                } else if chars[index] == '#' {
                    state = LexicalState::LineComment;
                    output.push(chars[index]);
                    index += 1;
                } else if chars[index] == '/' && chars.get(index + 1) == Some(&'*') {
                    state = LexicalState::BlockComment;
                    output.push('/');
                    output.push('*');
                    index += 2;
                } else if chars[index] == '\'' && chars.get(index + 1) == Some(&'\'') {
                    state = LexicalState::IndentedString;
                    output.push('\'');
                    output.push('\'');
                    index += 2;
                } else if index + legacy_chars.len() <= chars.len()
                    && chars[index..index + legacy_chars.len()] == legacy_chars
                    && (index == 0 || !is_nix_identifier_char(chars[index - 1]))
                    && chars
                        .get(index + legacy_chars.len())
                        .is_some_and(|character| is_nix_identifier_char(*character))
                {
                    output.push_str("config.");
                    index += legacy_chars.len();
                } else {
                    output.push(chars[index]);
                    index += 1;
                }
            }
            LexicalState::DoubleQuoted => {
                let character = chars[index];
                output.push(character);
                index += 1;
                if character == '\\' {
                    if let Some(escaped) = chars.get(index) {
                        output.push(*escaped);
                        index += 1;
                    }
                } else if character == '"' {
                    state = LexicalState::Normal;
                }
            }
            LexicalState::IndentedString => {
                if chars[index] == '\'' && chars.get(index + 1) == Some(&'\'') {
                    if chars
                        .get(index + 2)
                        .is_some_and(|next| matches!(next, '$' | '\\' | '\''))
                    {
                        output.push('\'');
                        output.push('\'');
                        index += 2;
                        continue;
                    }
                    output.push('\'');
                    output.push('\'');
                    index += 2;
                    state = LexicalState::Normal;
                } else {
                    output.push(chars[index]);
                    index += 1;
                }
            }
            LexicalState::LineComment => {
                let character = chars[index];
                output.push(character);
                index += 1;
                if character == '\n' {
                    state = LexicalState::Normal;
                }
            }
            LexicalState::BlockComment => {
                if chars[index] == '*' && chars.get(index + 1) == Some(&'/') {
                    output.push('*');
                    output.push('/');
                    index += 2;
                    state = LexicalState::Normal;
                } else {
                    output.push(chars[index]);
                    index += 1;
                }
            }
        }
    }

    let output = normalize_string_interpolations(&output);
    let changed = output != expression;
    (output, changed)
}

/// Normalize legacy references inside Nix string interpolations. The main
/// scanner intentionally treats string bodies as opaque, but `${...}` is an
/// embedded Nix expression and must be scanned as code. Escaped `\${` remains
/// literal; indented strings use the same rule for their interpolation body.
fn normalize_string_interpolations(expression: &str) -> String {
    let chars: Vec<char> = expression.chars().collect();
    let mut output = String::with_capacity(expression.len());
    let mut index = 0;
    let mut string_kind = None::<bool>; // false = double quoted, true = indented
    while index < chars.len() {
        if string_kind.is_none() {
            if chars[index] == '"' {
                string_kind = Some(false);
                output.push(chars[index]);
                index += 1;
                continue;
            }
            if chars[index] == '\'' && chars.get(index + 1) == Some(&'\'') {
                string_kind = Some(true);
                output.push('\'');
                output.push('\'');
                index += 2;
                continue;
            }
            output.push(chars[index]);
            index += 1;
            continue;
        }

        let indented = string_kind == Some(true);
        if !indented && chars[index] == '\\' {
            output.push(chars[index]);
            if let Some(next) = chars.get(index + 1) {
                output.push(*next);
                index += 2;
            } else {
                index += 1;
            }
            continue;
        }
        if indented && chars[index] == '\'' && chars.get(index + 1) == Some(&'\'') {
            if chars.get(index + 2) == Some(&'\\') {
                output.push('\'');
                output.push('\'');
                index += 2;
                output.push(chars[index]);
                index += 1;
                if let Some(escaped) = chars.get(index) {
                    output.push(*escaped);
                    index += 1;
                }
                continue;
            }
            if chars
                .get(index + 2)
                .is_some_and(|next| matches!(next, '$' | '\''))
            {
                output.push('\'');
                output.push('\'');
                index += 2;
                if let Some(escaped) = chars.get(index) {
                    output.push(*escaped);
                    index += 1;
                }
                continue;
            }
            output.push('\'');
            output.push('\'');
            index += 2;
            string_kind = None;
            continue;
        }
        if chars[index] == '$' && chars.get(index + 1) == Some(&'{') {
            let start = index + 2;
            if let Some(end) = interpolation_end(&chars, start) {
                let inner: String = chars[start..end].iter().collect();
                let (normalized, _) = normalize_custom_policy_expression(&inner);
                output.push_str("${");
                output.push_str(&normalized);
                output.push('}');
                index = end + 1;
                continue;
            }
        }
        if !indented && chars[index] == '"' {
            output.push(chars[index]);
            index += 1;
            string_kind = None;
        } else {
            output.push(chars[index]);
            index += 1;
        }
    }
    output
}

fn interpolation_end(chars: &[char], mut index: usize) -> Option<usize> {
    let mut depth = 1usize;
    let mut state = LexicalState::Normal;
    while index < chars.len() {
        match state {
            LexicalState::Normal => match chars[index] {
                '"' => {
                    state = LexicalState::DoubleQuoted;
                    index += 1;
                }
                '#' => {
                    state = LexicalState::LineComment;
                    index += 1;
                }
                '/' if chars.get(index + 1) == Some(&'*') => {
                    state = LexicalState::BlockComment;
                    index += 2;
                }
                '\'' if chars.get(index + 1) == Some(&'\'') => {
                    state = LexicalState::IndentedString;
                    index += 2;
                }
                '{' => {
                    depth += 1;
                    index += 1;
                }
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(index);
                    }
                    index += 1;
                }
                _ => index += 1,
            },
            LexicalState::DoubleQuoted => {
                if chars[index] == '\\' {
                    index += 2;
                } else if chars[index] == '$' && chars.get(index + 1) == Some(&'{') {
                    let nested_start = index + 2;
                    index = interpolation_end(chars, nested_start)? + 1;
                } else if chars[index] == '"' {
                    state = LexicalState::Normal;
                    index += 1;
                } else {
                    index += 1;
                }
            }
            LexicalState::IndentedString => {
                if chars[index] == '\'' && chars.get(index + 1) == Some(&'\'') {
                    if chars.get(index + 2) == Some(&'\\') {
                        index += 3;
                        if index < chars.len() {
                            index += 1;
                        }
                    } else if chars
                        .get(index + 2)
                        .is_some_and(|next| matches!(next, '$' | '\''))
                    {
                        index += 3;
                    } else {
                        state = LexicalState::Normal;
                        index += 2;
                    }
                } else if chars[index] == '$' && chars.get(index + 1) == Some(&'{') {
                    let nested_start = index + 2;
                    index = interpolation_end(chars, nested_start)? + 1;
                } else {
                    index += 1;
                }
            }
            LexicalState::LineComment => {
                if chars[index] == '\n' {
                    state = LexicalState::Normal;
                }
                index += 1;
            }
            LexicalState::BlockComment => {
                if chars[index] == '*' && chars.get(index + 1) == Some(&'/') {
                    state = LexicalState::Normal;
                    index += 2;
                } else {
                    index += 1;
                }
            }
        }
    }
    None
}

#[derive(Clone, Copy)]
enum LexicalState {
    Normal,
    DoubleQuoted,
    IndentedString,
    LineComment,
    BlockComment,
}

fn is_nix_identifier_char(character: char) -> bool {
    character.is_ascii_alphanumeric() || matches!(character, '_' | '-')
}

async fn run_post_finalize_derivation_side_effects(
    pool: &PgPool,
    derivations: &[FinalizedDerivation],
) {
    for derivation in derivations {
        match crate::builder::create_drv_gc_root(&derivation.drv_path, derivation.derivation_id)
            .await
        {
            Ok(true) => debug!(
                "📌 Rooted evaluated drv (id={}, drv={})",
                derivation.derivation_id, derivation.drv_path
            ),
            Ok(false) => warn!(
                "⚠️  Evaluated drv (id={}, drv={}) is not valid in the server store; \
                 remote builders may not be able to import it",
                derivation.derivation_id, derivation.drv_path
            ),
            Err(err) => warn!(
                "⚠️  Failed to create GC root for evaluated drv {} (id={}): {}",
                derivation.drv_path, derivation.derivation_id, err
            ),
        }

        let pool2 = pool.clone();
        let drv2 = derivation.drv_path.clone();
        let derivation_id = derivation.derivation_id;
        let limiter = closure_count_limiter();
        tokio::spawn(async move {
            let permit = match limiter.acquire_owned().await {
                Ok(permit) => permit,
                Err(err) => {
                    warn!(
                        "⚠️  Failed to acquire closure count permit for id={}: {}",
                        derivation_id, err
                    );
                    return;
                }
            };
            match count_closure_packages(&drv2).await {
                Ok((total, cached, closure_size_bytes)) => {
                    if let Err(err) =
                        set_closure_counts(&pool2, derivation_id, total, cached, closure_size_bytes)
                            .await
                    {
                        warn!(
                            "⚠️  Failed to store closure counts for id={}: {}",
                            derivation_id, err
                        );
                    } else {
                        info!(
                            "📦 closure id={}: {}/{} packages cached/local",
                            derivation_id, cached, total
                        );
                    }
                }
                Err(err) => warn!(
                    "⚠️  Failed to count closure packages for id={}: {}",
                    derivation_id, err
                ),
            }
            drop(permit);
        });
    }
}

async fn broadcast_queued_builds(
    cf_state: &crate::handlers::agent_request::CFState,
    commit_id: i32,
    queued_builds: &[QueuedBuild],
) {
    for build in queued_builds {
        debug!(
            "Queued build job {} for derivation {} ({})",
            build.build_job_id, build.derivation_id, build.system_name
        );
        crate::handlers::api::commits::broadcast_system_status(
            cf_state,
            commit_id,
            build.system_name.clone(),
            crate::handlers::api::commits::SystemEvalStatus::QueuedForBuild,
            None,
        )
        .await;
        crate::handlers::api::commits::broadcast_eval_log(
            cf_state,
            commit_id,
            format!("🚀 {}: build job queued", build.system_name),
        )
        .await;
    }
}

async fn handle_evaluation_attempt_failure(
    pool: &PgPool,
    cf_state: &crate::handlers::agent_request::CFState,
    commit: &Commit,
    attempt: i32,
    error: &str,
    failure_class: crate::models::retry_policy::RetryFailureClass,
) -> Result<()> {
    // SECURITY: This function logs and persists the failure. Redact once at
    // entry so no branch can expose the raw evaluator diagnostic.
    let error = crate::security::snapshot_redaction::redact_evaluation_error(error);
    error!(
        "❌ Failed to evaluate commit {}: {}",
        commit.git_commit_hash, error
    );

    match mark_commit_evaluation_failed(pool, commit.id, &error, attempt, failure_class).await {
        Err(mark_err) => {
            crate::handlers::api::commits::cleanup_eval_channel(cf_state, commit.id).await;
            return Err(mark_err).with_context(|| {
                format!(
                    "Failed to mark commit {} evaluation failed (attempt {})",
                    commit.git_commit_hash, attempt
                )
            });
        }
        Ok(EvalFailureOutcome::SupersededOrCancelled) => {
            let cancel_outcome =
                crate::queries::commits::finalize_requested_commit_evaluation_cancellation(
                    pool, commit.id, attempt,
                )
                .await
                .with_context(|| {
                    format!(
                        "Failed to finalize cancellation for commit {} attempt {}",
                        commit.id, attempt
                    )
                })?;

            if matches!(
                cancel_outcome,
                EvalCancellationOutcome::Cancelled | EvalCancellationOutcome::AlreadyCancelled
            ) {
                if let Err(err) =
                    crate::queries::commits::cleanup_partial_derivations_for_commit(pool, commit.id)
                        .await
                {
                    warn!(
                        "Failed to clean partial derivations for cancelled commit {}: {}",
                        commit.id, err
                    );
                }
                crate::handlers::api::commits::broadcast_eval_status(
                    cf_state,
                    commit.id,
                    "cancelled".to_string(),
                    Some("Evaluation cancelled by user".to_string()),
                )
                .await;
                crate::handlers::api::commits::broadcast_eval_log(
                    cf_state,
                    commit.id,
                    "🚫 Evaluation cancelled by user".to_string(),
                )
                .await;
            } else {
                warn!(
                    "Commit {} evaluation superseded; failure not recorded for attempt {}",
                    commit.git_commit_hash, attempt
                );
            }
        }
        Ok(EvalFailureOutcome::RetryScheduled) => {
            if let Err(cache_err) = update_commit_metadata_cache(pool, commit.id, &[], true).await {
                error!(
                    "❌ Failed to update commit metadata cache for {}: {}",
                    commit.git_commit_hash, cache_err
                );
            }

            info!(
                "Commit {} evaluation will be retried (attempt {})",
                commit.git_commit_hash, attempt
            );

            crate::handlers::api::commits::broadcast_eval_status(
                cf_state,
                commit.id,
                "retrying".to_string(),
                Some(format!("Evaluation will be retried: {}", error)),
            )
            .await;
            crate::handlers::api::commits::broadcast_eval_log(
                cf_state,
                commit.id,
                format!(
                    "🔄 Evaluation will be retried (attempt {}): {}",
                    attempt, error
                ),
            )
            .await;
        }
        Ok(EvalFailureOutcome::PermanentlyFailed) => {
            if let Err(cache_err) = update_commit_metadata_cache(pool, commit.id, &[], true).await {
                error!(
                    "❌ Failed to update commit metadata cache for {}: {}",
                    commit.git_commit_hash, cache_err
                );
            }

            crate::handlers::api::commits::broadcast_eval_status(
                cf_state,
                commit.id,
                "failed".to_string(),
                Some(format!("Evaluation failed: {}", error)),
            )
            .await;
            crate::handlers::api::commits::broadcast_eval_log(
                cf_state,
                commit.id,
                format!("❌ Evaluation permanently failed: {}", error),
            )
            .await;
        }
    }

    crate::handlers::api::commits::cleanup_eval_channel(cf_state, commit.id).await;
    Ok(())
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
            let Some(raw_packages) = cfg.get("packages").and_then(|value| value.as_array()) else {
                warn!(
                    "Skipping require_packages policy '{}' ({}): config.packages must be a non-empty string array",
                    record.name, record.id
                );
                return None;
            };
            let mut packages = Vec::with_capacity(raw_packages.len());
            for (index, value) in raw_packages.iter().enumerate() {
                let Some(package) = value
                    .as_str()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                else {
                    warn!(
                        "Skipping require_packages policy '{}' ({}): config.packages[{}] must be a non-empty string",
                        record.name, record.id, index
                    );
                    return None;
                };
                packages.push(package.to_string());
            }
            if packages.is_empty() {
                warn!(
                    "Skipping require_packages policy '{}' ({}): config.packages cannot be empty",
                    record.name, record.id
                );
                return None;
            }
            Some(DeploymentPolicy::RequirePackages { packages, strict })
        }
        "custom_check" => {
            use crate::models::deployment_policies::{PolicyRule, RuleMode};

            // Determine which shape this record uses: multi-rule (rules[]) or legacy (expression).
            let raw_rules = cfg
                .get("rules")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();

            let has_rules = !raw_rules.is_empty();
            let raw_expression = cfg
                .get("expression")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_default();

            if !has_rules && raw_expression.is_empty() {
                warn!(
                    "Skipping custom_check policy '{}' ({}): neither config.expression nor config.rules[] is present",
                    record.name, record.id
                );
                return None;
            }

            let strict = cfg.get("strict").and_then(|v| v.as_bool()).unwrap_or(false);

            let mode = match cfg.get("mode").and_then(|v| v.as_str()).unwrap_or("all") {
                "any" => RuleMode::Any,
                _ => RuleMode::All,
            };

            if has_rules {
                // Multi-rule path: parse each rule from config.rules[].
                let mut parsed_rules: Vec<PolicyRule> = Vec::with_capacity(raw_rules.len());
                for (i, rule) in raw_rules.iter().enumerate() {
                    let rule_obj = match rule.as_object() {
                        Some(o) => o,
                        None => {
                            warn!(
                                "Skipping custom_check policy '{}' ({}): rules[{}] is not an object",
                                record.name, record.id, i
                            );
                            return None;
                        }
                    };
                    let expression = match rule_obj.get("expression").and_then(|v| v.as_str()) {
                        Some(e) => normalize_custom_policy_expression(e).0,
                        None => {
                            warn!(
                                "Skipping custom_check policy '{}' ({}): rules[{}] missing expression",
                                record.name, record.id, i
                            );
                            return None;
                        }
                    };
                    let field_name = match rule_obj.get("field_name").and_then(|v| v.as_str()) {
                        Some(f) => f.to_string(),
                        None => {
                            warn!(
                                "Skipping custom_check policy '{}' ({}): rules[{}] missing field_name",
                                record.name, record.id, i
                            );
                            return None;
                        }
                    };
                    let description = rule_obj
                        .get("description")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| format!("rule_{}", i));
                    let rule_strict = rule_obj
                        .get("strict")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(true);
                    parsed_rules.push(PolicyRule {
                        expression,
                        description,
                        field_name,
                        strict: rule_strict,
                    });
                }

                let description = cfg
                    .get("description")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                    .or_else(|| record.description.clone())
                    .unwrap_or_else(|| format!("Multi-rule policy: {}", record.name));

                Some(DeploymentPolicy::CustomCheck {
                    expression: String::new(),
                    description,
                    field_name: record.name.clone(),
                    strict,
                    rules: parsed_rules,
                    mode,
                })
            } else {
                // Legacy single-expression path.
                let (expression, normalized_legacy_ref) =
                    normalize_custom_policy_expression(&raw_expression);
                if normalized_legacy_ref {
                    warn!(
                        "Normalized legacy custom_check expression for policy '{}' ({}): replaced `cfg.config.` with `config.`",
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

                Some(DeploymentPolicy::CustomCheck {
                    expression,
                    description,
                    field_name,
                    strict,
                    rules: vec![],
                    mode: RuleMode::All,
                })
            }
        }
        "require_cve_check" => {
            match serde_json::from_value::<crate::models::deployment_policies::CveCheckConfig>(
                cfg.clone(),
            ) {
                Ok(config) => Some(DeploymentPolicy::RequireCveCheck { config }),
                Err(err) => {
                    warn!(
                        "Skipping require_cve_check policy '{}' ({}): invalid config: {}",
                        record.name, record.id, err
                    );
                    None
                }
            }
        }
        "time_window" => {
            match serde_json::from_value::<crate::models::deployment_policies::TimeWindowConfig>(
                cfg.clone(),
            ) {
                Ok(config) => Some(DeploymentPolicy::TimeWindow { config }),
                Err(err) => {
                    warn!(
                        "Skipping time_window policy '{}' ({}): invalid config: {}",
                        record.name, record.id, err
                    );
                    None
                }
            }
        }
        "require_approvals" => {
            match serde_json::from_value::<crate::models::deployment_policies::ApprovalConfig>(
                cfg.clone(),
            ) {
                Ok(config) => Some(DeploymentPolicy::RequireApprovals { config }),
                Err(err) => {
                    warn!(
                        "Skipping require_approvals policy '{}' ({}): invalid config: {}",
                        record.name, record.id, err
                    );
                    None
                }
            }
        }
        "canary_rollout" => {
            match serde_json::from_value::<crate::models::deployment_policies::CanaryConfig>(
                cfg.clone(),
            ) {
                Ok(config) => Some(DeploymentPolicy::CanaryRollout { config }),
                Err(err) => {
                    warn!(
                        "Skipping canary_rollout policy '{}' ({}): invalid config: {}",
                        record.name, record.id, err
                    );
                    None
                }
            }
        }
        "cve_threshold" => {
            match serde_json::from_value::<crate::models::deployment_policies::CveThresholdConfig>(
                cfg.clone(),
            ) {
                Ok(config) => Some(DeploymentPolicy::CveThreshold { config }),
                Err(err) => {
                    warn!(
                        "Skipping cve_threshold policy '{}' ({}): invalid config: {}",
                        record.name, record.id, err
                    );
                    None
                }
            }
        }
        "composite" => {
            match crate::models::deployment_policies::deserialize_policy_type_config(
                &record.policy_type,
                cfg,
            ) {
                Ok(Some(config)) => Some(DeploymentPolicy::Composite { config }),
                Ok(None) => unreachable!("composite validator returned a non-composite result"),
                Err(err) => {
                    warn!(
                        "Skipping composite policy '{}' ({}): {}",
                        record.name, record.id, err
                    );
                    None
                }
            }
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

fn is_explicit_no_enforcement_policy(
    record: &crate::models::deployment_policies::DeploymentPolicyRecord,
) -> bool {
    record.policy_type == "custom_check"
        && record.config.get("expression").is_none()
        && record.config.get("mode").and_then(|value| value.as_str()) == Some("all")
        && record
            .config
            .get("rules")
            .and_then(|value| value.as_array())
            .is_some_and(Vec::is_empty)
}

fn parse_enforced_policy_record(
    record: &crate::models::deployment_policies::DeploymentPolicyRecord,
) -> std::result::Result<Option<DeploymentPolicy>, EnforcedPolicyLoadSafetyError> {
    match parse_deployment_policy_record(record) {
        Some(policy) => Ok(Some(policy)),
        None if is_explicit_no_enforcement_policy(record) => Ok(None),
        None => Err(EnforcedPolicyLoadSafetyError(format!(
            "enforced policy version {} of type {:?} has malformed or unsupported config; refusing evaluation",
            record.id, record.policy_type
        ))),
    }
}

async fn load_deployment_policies_for_eval(
    pool: &PgPool,
    flake_id: i32,
) -> anyhow::Result<Vec<DeploymentPolicy>> {
    match list_enabled_policies_for_flake(pool, flake_id).await {
        Ok(records) => {
            let all_policies = records
                .iter()
                .map(parse_enforced_policy_record)
                .collect::<std::result::Result<Vec<_>, _>>()?
                .into_iter()
                .flatten()
                .collect::<Vec<_>>();

            // Only pass Nix-evaluated policies to the evaluator.
            // RequireCveCheck policies are handled in the deployment manager.
            let mut policies: Vec<DeploymentPolicy> = all_policies
                .into_iter()
                .filter(|p| p.is_nix_evaluated())
                .collect();

            if policies.is_empty() {
                warn!(
                    "No valid Nix-evaluated deployment policies found in DB, falling back to strict CF agent check"
                );
                // Use strict mode in fallback to enforce core security policy even in error scenarios.
                // This ensures systems without the agent package cannot pass evaluation when policy
                // loading fails, maintaining the "always enforce core policy" safety model.
                policies.push(DeploymentPolicy::RequireCrystalForgeAgent { strict: true });
            }

            Ok(policies)
        }
        Err(err) => {
            error!(
                "Failed to load deployment policies from DB for evaluation: {:#}. Falling back to strict CF agent check",
                err
            );
            // Use strict mode in fallback to enforce core security policy even in error scenarios.
            // This ensures systems without the agent package cannot pass evaluation when policy
            // loading fails, maintaining the "always enforce core policy" safety model.
            Ok(vec![DeploymentPolicy::RequireCrystalForgeAgent {
                strict: true,
            }])
        }
    }
}

/// Build a per-configuration policy map for a flake's evaluation run.
///
/// For each active Crystal Forge system in the flake, the effective policy set
/// is the union of:
/// - Policies assigned directly through `system_policies`
/// - Policies inherited from the system's environment through `environment_policies`
///
/// If two active systems share the same NixOS configuration name but have
/// *different* effective policy ID sets, this function returns an error with
/// an actionable message — silently unioning the sets would re-introduce the
/// cross-environment policy leak we are trying to eliminate.
///
/// Systems with zero assigned policies produce *no entry* in the returned map.
/// Use `policies_for_config(map, name)` to safely get an empty slice for those.
async fn load_policies_by_configuration_for_eval(
    pool: &PgPool,
    flake_id: i32,
) -> anyhow::Result<PoliciesByConfiguration> {
    use std::collections::{BTreeMap, BTreeSet};

    let system_rows = sqlx::query_as::<_, (uuid::Uuid, String)>(
        r#"
        SELECT s.id, COALESCE(NULLIF(BTRIM(s.system_configuration_name), ''), s.hostname)
        FROM systems s
        WHERE s.flake_id = $1 AND s.is_active = TRUE
        ORDER BY 2, 1
        "#,
    )
    .bind(flake_id)
    .fetch_all(pool)
    .await
    .context("Failed to load systems for effective policy evaluation")?;

    let mut resolved_systems = Vec::with_capacity(system_rows.len());
    let mut policy_version_ids = BTreeSet::new();
    let mut invalid_configurations = BTreeSet::new();

    // Batch-resolve all systems in one transaction (~10 queries total),
    // replacing the previous per-system N+1 loop of full resolver invocations.
    let system_id_vec: Vec<uuid::Uuid> = system_rows.iter().map(|(id, _)| *id).collect();
    let batch_outcomes =
        resolve_systems_effective_policies_for_evaluation_batch(pool, &system_id_vec)
            .await
            .context("Batch policy resolution for evaluation failed")?;

    for (system_id, config_name) in system_rows {
        if invalid_configurations.contains(&config_name) {
            continue;
        }
        let outcome = match batch_outcomes.get(&system_id) {
            Some(o) => match o {
                ResolutionOutcome::Resolved(set) => set.policies.clone(),
                ResolutionOutcome::Conflict(conflicts) => {
                    warn!(
                        %system_id,
                        %config_name,
                        conflicts = ?conflicts,
                        "Compliance policy conflict; preserving legacy behavior and evaluating configuration without compliance gates"
                    );
                    invalid_configurations.insert(config_name.clone());
                    resolved_systems
                        .retain(|(_, existing_config, _)| existing_config != &config_name);
                    continue;
                }
            },
            None => {
                // System not in batch result — no active assignments (empty policy set).
                vec![]
            }
        };

        // Report-only policies are deliberately nonblocking at this loader
        // boundary. Every record retained below must parse safely or fail the
        // evaluation deterministically.
        let enforced = outcome
            .into_iter()
            .filter(|policy| matches!(policy.effective_mode, AssignmentMode::Enforce))
            .collect::<Vec<_>>();
        policy_version_ids.extend(enforced.iter().map(|policy| policy.policy_version_id));
        resolved_systems.push((system_id, config_name, enforced));
    }

    let policy_version_ids = policy_version_ids.into_iter().collect::<Vec<_>>();
    let policies_by_version =
        get_deployment_policies_by_versions(pool, &policy_version_ids).await?;

    let mut map: PoliciesByConfiguration = BTreeMap::new();
    // A configuration can be evaluated once only when the Nix-evaluation
    // policy set is identical. The resolver's complete effective-set digest
    // intentionally includes report-only and operational policies, which do
    // not affect nix-eval-jobs and therefore must not block this map.
    let mut evaluation_digest_by_config: BTreeMap<String, String> = BTreeMap::new();
    for (_system_id, config_name, effective) in resolved_systems {
        let mut assigned = Vec::new();
        for effective_policy in effective {
            let Some(record) = policies_by_version.get(&effective_policy.policy_version_id) else {
                return Err(anyhow::Error::new(EnforcedPolicyLoadSafetyError(format!(
                    "effective policy version {} was not found",
                    effective_policy.policy_version_id
                ))));
            };
            let mut record = record.clone();

            if !record.enabled {
                continue;
            }

            if !effective_policy.effective_config.is_null() {
                record.config = effective_policy.effective_config;
            }

            if let Some(policy) = parse_enforced_policy_record(&record)? {
                if policy.is_nix_evaluated()
                    && !matches!(policy, DeploymentPolicy::RequireCrystalForgeAgent { .. })
                {
                    assigned.push(AssignedPolicy {
                        policy_id: effective_policy.policy_version_id,
                        policy_name: record.name,
                        policy,
                    });
                }
            }
        }

        // Sort the actual evaluator input by portable version identity so the
        // generated per-policy fields and its digest are canonical. Keep an
        // empty set in the comparison map: a shared configuration with one
        // system having no Nix gates and another having Nix gates is a real
        // semantic conflict, not permission to apply the latter's gates to
        // both systems.
        assigned.sort_by_key(|policy| policy.policy_id);
        let evaluation_digest = evaluation_policy_digest(&assigned);

        if let Some(existing_digest) = evaluation_digest_by_config.get(&config_name) {
            if existing_digest != &evaluation_digest {
                return Err(anyhow::Error::new(EnforcedPolicyLoadSafetyError(format!(
                    "Configuration {:?} resolves to different Nix evaluation policy semantics across systems ({} vs {})",
                    config_name, existing_digest, evaluation_digest
                ))));
            }
        } else {
            evaluation_digest_by_config.insert(config_name.clone(), evaluation_digest);
            if !assigned.is_empty() {
                map.insert(config_name, assigned);
            }
        }
    }

    info!(
        flake_id,
        unique_policies = map
            .values()
            .flat_map(|policies| policies.iter())
            .map(|policy| policy.policy_id)
            .collect::<BTreeSet<_>>()
            .len(),
        registered_configurations_with_policies = map.len(),
        "effective_policies_by_configuration_loaded"
    );

    return Ok(map);
}

#[derive(Debug)]
struct EnforcedPolicyLoadSafetyError(String);

impl std::fmt::Display for EnforcedPolicyLoadSafetyError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for EnforcedPolicyLoadSafetyError {}

fn classify_policy_loader_failure(
    error: &anyhow::Error,
) -> crate::models::retry_policy::RetryFailureClass {
    if error
        .chain()
        .any(|cause| cause.is::<EnforcedPolicyLoadSafetyError>())
    {
        crate::models::retry_policy::RetryFailureClass::Deterministic
    } else {
        crate::models::retry_policy::RetryFailureClass::Transient
    }
}

/// Hash only the policies that can affect the Nix evaluation for one
/// configuration. This is deliberately distinct from the resolver's complete
/// effective-set digest, which is used by compliance and deployment consumers.
fn evaluation_policy_digest(assigned: &[AssignedPolicy]) -> String {
    let policies = assigned
        .iter()
        .map(|policy| {
            serde_json::json!({
                "policy_version_id": policy.policy_id,
                "policy": policy.policy,
            })
        })
        .collect::<Vec<_>>();
    semantic_digest(&serde_json::Value::Array(policies))
}

#[allow(dead_code)]
async fn load_policies_by_configuration_for_eval_legacy(
    pool: &PgPool,
    flake_id: i32,
) -> anyhow::Result<PoliciesByConfiguration> {
    use std::collections::{BTreeMap, BTreeSet, HashMap};

    let rows = list_policy_rows_by_configuration_for_flake(pool, flake_id).await?;

    // Group raw rows by configuration name, collecting (policy_id, record) pairs.
    // The SQL already orders by (configuration_name, policy_id) so insertion order
    // is deterministic; using a BTreeMap preserves that order in the output.
    let mut raw: BTreeMap<
        String,
        Vec<(
            uuid::Uuid,
            crate::queries::deployment_policies::ConfigPolicyRow,
        )>,
    > = BTreeMap::new();
    for row in rows {
        raw.entry(row.configuration_name.clone())
            .or_default()
            .push((row.policy_id, row));
    }

    let mut map: PoliciesByConfiguration = BTreeMap::new();

    for (config_name, policy_rows) in raw {
        // Deduplicate by policy_id (the UNION in SQL handles most cases but
        // two systems in different environments can both reference the same
        // environment if environment membership changes mid-request).
        let mut seen_ids: BTreeSet<uuid::Uuid> = BTreeSet::new();
        let mut assigned: Vec<AssignedPolicy> = Vec::new();

        for (policy_id, row) in policy_rows {
            if !seen_ids.insert(policy_id) {
                continue; // duplicate — skip
            }
            let record = row.as_policy_record();
            if let Some(parsed) = parse_deployment_policy_record(&record) {
                if parsed.is_nix_evaluated() {
                    // require_cf_agent is handled by the unconditional
                    // global invariant.  A legacy assignment would
                    // produce a duplicate CF-agent result column in
                    // the policy matrix, so it is filtered out here.
                    // The global check (cfAgentEnabled) is always
                    // emitted and enforced regardless of assignments.
                    if matches!(parsed, DeploymentPolicy::RequireCrystalForgeAgent { .. }) {
                        warn!(
                            "Ignoring legacy require_cf_agent assignment {} ({}) — \
                             CF-agent enablement is enforced globally",
                            policy_id, row.name,
                        );
                        continue;
                    }
                    assigned.push(AssignedPolicy {
                        policy_id,
                        policy_name: row.name.clone(),
                        policy: parsed,
                    });
                }
            }
        }

        if !assigned.is_empty() {
            map.insert(config_name, assigned);
        }
    }

    // Conflict detection: multiple *active* systems sharing a configuration
    // name but with different effective policy sets.
    // We re-query registered names to detect systems that were collapsed above.
    // The per-config deduplication already merged identical sets; we only need
    // to check whether two systems independently produced *different* sets.
    // Since the SQL UNION already merged rows from multiple systems with the
    // same config name, a conflict manifests as a discrepancy between what
    // one system would have contributed vs. another. We detect this by checking
    // whether any two `scoped_systems` rows with the same config name have
    // different effective policy-ID collections — which requires a second query.
    // For now we detect only the case where the merged set cannot be
    // unambiguously attributed to a single environment. A future enhancement
    // can query system-level breakdowns.
    //
    // NOTE: The UNION of environment_policies and system_policies with the
    // same config name is intentional when two systems in the same environment
    // have different DIRECT assignments — that's the dedup case above. A
    // genuine conflict (different environments, different required policies)
    // is caught by a separate verification pass below.
    {
        // Build a map: config_name → set of unique policy-id sets contributed
        // by individual systems (i.e. per-system effective policy sets).
        // This requires per-system breakdown.
        #[derive(Debug)]
        struct SystemPolicySet {
            system_id: uuid::Uuid,
            policy_ids: BTreeSet<uuid::Uuid>,
        }

        let system_rows = sqlx::query_as::<_, (uuid::Uuid, String, Option<uuid::Uuid>)>(
            r#"
            SELECT s.id, COALESCE(NULLIF(BTRIM(s.system_configuration_name), ''), s.hostname),
                   s.environment_id
            FROM systems s
            WHERE s.flake_id = $1 AND s.is_active = TRUE
            "#,
        )
        .bind(flake_id)
        .fetch_all(pool)
        .await
        .context("Failed to load systems for conflict detection")?;

        // For each system, load its effective policy IDs.
        let mut per_config_systems: HashMap<String, Vec<SystemPolicySet>> = HashMap::new();
        for (system_id, config_name, env_id) in &system_rows {
            let direct_ids: BTreeSet<uuid::Uuid> = sqlx::query_scalar::<_, uuid::Uuid>(
                "SELECT policy_id FROM system_policies WHERE system_id = $1",
            )
            .bind(system_id)
            .fetch_all(pool)
            .await
            .unwrap_or_default()
            .into_iter()
            .collect();

            let env_ids: BTreeSet<uuid::Uuid> = if let Some(eid) = env_id {
                sqlx::query_scalar::<_, uuid::Uuid>(
                    "SELECT policy_id FROM environment_policies WHERE environment_id = $1",
                )
                .bind(eid)
                .fetch_all(pool)
                .await
                .unwrap_or_default()
                .into_iter()
                .collect()
            } else {
                BTreeSet::new()
            };

            let mut all_ids: BTreeSet<uuid::Uuid> = direct_ids;
            all_ids.extend(env_ids);

            per_config_systems
                .entry(config_name.clone())
                .or_default()
                .push(SystemPolicySet {
                    system_id: *system_id,
                    policy_ids: all_ids,
                });
        }

        for (config_name, systems) in &per_config_systems {
            if systems.len() < 2 {
                continue;
            }
            // Check whether all systems have the same effective policy set.
            let first_set = &systems[0].policy_ids;
            for other in &systems[1..] {
                if &other.policy_ids != first_set {
                    anyhow::bail!(
                        "Configuration {:?} is assigned to active systems with different policy sets: \
                         system {} has policies {:?}, while system {} has policies {:?}. \
                         Use distinct system_configuration_name values or align their assigned policies.",
                        config_name,
                        systems[0].system_id,
                        first_set.iter().collect::<Vec<_>>(),
                        other.system_id,
                        other.policy_ids.iter().collect::<Vec<_>>(),
                    );
                }
            }
        }
    }

    info!(
        flake_id,
        unique_policies = map
            .values()
            .flat_map(|v| v.iter())
            .map(|a| a.policy_id)
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        registered_configurations_with_policies = map.len(),
        "policies_by_configuration_loaded"
    );
    for (config, policies) in &map {
        debug!(
            configuration = %config,
            assigned_policy_count = policies.len(),
            "per_configuration_policy_assignment"
        );
    }

    Ok(map)
}

/// Load enabled `require_cve_check` policies from the database.
/// Called by the deployment manager to evaluate post-build CVE gates.
pub async fn load_cve_policies(pool: &PgPool) -> Vec<DeploymentPolicy> {
    match list_enabled_deployment_policies(pool).await {
        Ok(records) => records
            .iter()
            .filter_map(parse_deployment_policy_record)
            .filter(|p| matches!(p, DeploymentPolicy::RequireCveCheck { .. }))
            .collect(),
        Err(err) => {
            error!("Failed to load CVE deployment policies from DB: {:#}", err);
            vec![]
        }
    }
}

/// Spawn all server background tasks and register controllable jobs in the
/// provided [`BackgroundJobRegistry`].
///
/// The registry is stored on server state so the Admin → Background Jobs tab
/// (TASK-336.5) can expose live job status and runtime controls (enable/disable,
/// run-now) without a heavyweight scheduler.
pub fn spawn_background_tasks(
    cfg: CrystalForgeConfig,
    pool: PgPool,
    cf_state: Arc<crate::handlers::agent_request::CFState>,
    queue_notifier: Arc<QueueNotifier>,
    job_registry: BackgroundJobRegistry,
) {
    let flake_pool = pool.clone();
    let commit_pool = pool.clone();
    let target_pool = pool.clone();
    let deployment_pool = pool.clone();
    let artifact_pool = pool.clone();
    let build_log_pool = pool.clone();
    let cve_pool = pool.clone();

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
    tokio::spawn(run_builder_recovery_loop(
        target_pool,
        cfg.builder.heartbeat_interval,
        queue_notifier,
    ));
    tokio::spawn(run_commit_artifact_hydration_loop(artifact_pool));
    tokio::spawn(run_build_log_retention_loop(
        build_log_pool,
        cfg.server.build_log_retention_days,
        cfg.server.failed_build_log_retention_days,
    ));

    // Bounded time-driven reconciliation for systems that stop heartbeating
    // and flakes stuck syncing past the staleness threshold — see
    // `tasks::attention_reconciliation` for why these need a periodic sweep
    // rather than only a request-triggered hook.
    let attention_reconciliation_pool = pool.clone();
    tokio::spawn(
        crate::tasks::attention_reconciliation::run_attention_reconciliation_loop(
            attention_reconciliation_pool,
        ),
    );
    let attention_cleanup_pool = pool.clone();
    tokio::spawn(
        crate::tasks::attention_reconciliation::run_attention_cleanup_loop(attention_cleanup_pool),
    );

    let commit_cache_pool = pool.clone();
    tokio::spawn(run_commit_cache_gc_loop(
        commit_cache_pool,
        cfg.server.commit_cache_retention_days,
    ));

    let notification_email_pool = pool.clone();
    tokio::spawn(
        crate::tasks::user_notification_email::run_user_notification_email_loop(
            notification_email_pool,
            cfg.server.clone(),
        ),
    );

    let session_cleanup_pool = pool.clone();
    let session_retention_days = cfg.server.session_retention_days;
    tokio::spawn(run_session_retention_loop(
        session_cleanup_pool,
        session_retention_days as i64,
    ));

    tokio::spawn(spawn_deployment_policy_manager(
        cfg.clone(),
        deployment_pool,
    ));

    // --- CVE scan background job ---
    // The job handle is registered in the registry so the Admin Background Jobs
    // tab (TASK-336.5) can list it, toggle enabled/disabled, and trigger run-now.
    // vulnix poll_interval comes from [vulnix] config (default: 60 s).
    let vulnix_poll_interval = cfg.get_vulnix_config().poll_interval;
    // The worker starts enabled because the persisted scan schedule policy is
    // the operational control for post-build and cadence-driven scans. Runtime
    // controls remain available for the future Admin Background Jobs surface.
    let (cve_job_handle, cve_run_now_rx) = new_cve_scan_background_job(vulnix_poll_interval);
    let cve_job_for_task = cve_job_handle.clone();
    let registry_for_spawn = job_registry.clone();
    tokio::spawn(async move {
        registry_for_spawn.register(cve_job_handle).await;
        run_cve_scan_loop(cve_pool, cve_job_for_task, cve_run_now_rx).await;
    });
}

fn new_cve_scan_background_job(
    poll_interval: std::time::Duration,
) -> (BackgroundJobHandle, tokio::sync::watch::Receiver<u64>) {
    BackgroundJobHandle::new("cve_scan", "CVE Scan", poll_interval, true)
}

async fn run_session_retention_loop(pool: PgPool, retention_days: i64) {
    let mut ticker = tokio::time::interval(std::time::Duration::from_secs(24 * 60 * 60));
    loop {
        match crate::queries::auth_identity::cleanup_retained_user_sessions(
            &pool,
            retention_days,
            1_000,
        )
        .await
        {
            Ok(deleted) if deleted > 0 => {
                tracing::info!(deleted, "cleaned retained user session records")
            }
            Ok(_) => {}
            Err(err) => tracing::warn!(%err, "user session retention cleanup failed"),
        }
        ticker.tick().await;
    }
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
                    match sync_all_watched_flakes_commits_with_ids(&pool, &db_flakes, &flake_ids)
                        .await
                    {
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

    // Recover any DryRunComplete derivations that have no build job, which can
    // happen when the build-preparation task failed or the server restarted
    // between derivation persistence and build-job activation.
    match recover_orphaned_derivation_build_jobs(&pool).await {
        Ok(count) if count > 0 => {
            info!(
                "🔄 Startup: queued {} orphaned build-eligible derivations",
                count
            );
            queue_notifier.notify_build_queue();
        }
        Ok(_) => {}
        Err(e) => error!("❌ Failed to recover orphaned derivation build jobs: {}", e),
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

        let due_delay = match next_evaluation_available_at(&pool).await {
            Ok(Some(available_at)) => evaluation_due_delay(chrono::Utc::now(), available_at),
            Ok(None) => interval,
            Err(e) => {
                warn!("Failed to load next evaluation due time: {e:#}");
                interval
            }
        };
        let due_wakeup = time::sleep(due_delay);
        tokio::pin!(due_wakeup);

        // Wait for a notification, durable retry due time, or fallback ticker.
        tokio::select! {
            _ = ticker.tick() => {
                debug!("⏰ Eval loop: periodic tick (fallback polling)");
            }
            _ = queue_notifier.wait_for_eval_work() => {
                debug!("🔔 Eval loop: notified of new work");
            }
            _ = &mut due_wakeup => {
                debug!("⏰ Eval loop: next delayed evaluation is due");
            }
        }
    }
}

fn evaluation_due_delay(
    now: chrono::DateTime<chrono::Utc>,
    available_at: chrono::DateTime<chrono::Utc>,
) -> Duration {
    available_at
        .signed_duration_since(now)
        .to_std()
        .unwrap_or(Duration::ZERO)
}

fn builder_stale_timeout_secs(heartbeat_interval: Duration) -> i64 {
    let heartbeat_secs = heartbeat_interval.as_secs().max(15);
    ((heartbeat_secs * 3).max(60)) as i64
}

async fn recover_orphaned_build_jobs_cycle(
    pool: &PgPool,
    stale_timeout_secs: i64,
    queue_notifier: &Arc<QueueNotifier>,
    reason: &str,
) -> Result<()> {
    let stale = mark_stale_builders_offline(pool, stale_timeout_secs).await?;
    if stale > 0 {
        warn!(
            "🧹 Marked {} stale builders offline (timeout={}s)",
            stale, stale_timeout_secs
        );
    }

    let recovered = requeue_orphaned_building_jobs_with_reason(pool, reason).await?;
    if !recovered.is_empty() {
        warn!(
            reason = reason,
            recovered_jobs = recovered.len(),
            "🔄 Re-queued orphaned build jobs stuck in building"
        );
        queue_notifier.notify_build_queue();
    }

    Ok(())
}

async fn run_builder_recovery_loop(
    pool: PgPool,
    heartbeat_interval: Duration,
    queue_notifier: Arc<QueueNotifier>,
) {
    let tick_secs = heartbeat_interval.as_secs().max(15);
    let stale_timeout_secs = builder_stale_timeout_secs(heartbeat_interval);
    info!(
        "🔁 Starting builder recovery loop (tick={}s, stale_timeout={}s)",
        tick_secs, stale_timeout_secs
    );

    if let Err(err) = recover_orphaned_build_jobs_cycle(
        &pool,
        stale_timeout_secs,
        &queue_notifier,
        "startup builder recovery",
    )
    .await
    {
        error!("❌ Initial builder recovery cycle failed: {:#}", err);
    }

    let mut ticker = interval(Duration::from_secs(tick_secs));
    loop {
        if let Err(err) = recover_orphaned_build_jobs_cycle(
            &pool,
            stale_timeout_secs,
            &queue_notifier,
            "runtime builder liveness recovery",
        )
        .await
        {
            error!("❌ Builder recovery cycle failed: {:#}", err);
        }

        // Periodically recover derivations whose build-preparation task failed.
        // This runs regardless of service restarts so a build job is eventually
        // created without requiring manual intervention or a service restart.
        match recover_orphaned_derivation_build_jobs(&pool).await {
            Ok(count) if count > 0 => {
                info!(
                    "🔄 Periodic recovery: queued {} orphaned build-eligible derivations",
                    count
                );
                queue_notifier.notify_build_queue();
            }
            Ok(_) => {}
            Err(e) => error!("❌ Periodic build-preparation recovery failed: {}", e),
        }

        ticker.tick().await;
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
        let attempt = match mark_commit_evaluation_started(pool, commit.id).await {
            Ok(EvalStartOutcome::Started { attempt }) => attempt,
            Ok(EvalStartOutcome::NoLongerPending) => {
                debug!(
                    "⏭️ Eval start race for commit {} ({}): another worker/loop iteration already took pending",
                    commit.id, commit.git_commit_hash
                );
                return Ok(());
            }
            Err(e) => {
                error!(
                    "❌ Could not mark commit {} evaluation started: {}",
                    commit.git_commit_hash, e
                );
                return Ok(());
            }
        };

        // Get flake info (post-claim; failures now go through retry/defer path)
        let flake = match commit.get_flake(&pool).await {
            Ok(flake) => flake,
            Err(e) => {
                error!(
                    "❌ Failed to get flake for commit {}: {}",
                    commit.git_commit_hash, e
                );
                let _ = mark_commit_evaluation_failed(
                    pool,
                    commit.id,
                    &e.to_string(),
                    attempt,
                    crate::models::retry_policy::RetryFailureClass::Deterministic,
                )
                .await;
                return Ok(());
            }
        };

        // Load Crystal Forge config to get build settings (post-claim retry/defer path)
        let cfg = match CrystalForgeConfig::load() {
            Ok(cfg) => cfg,
            Err(e) => {
                error!("❌ Failed to load config: {}", e);
                let _ = mark_commit_evaluation_failed(
                    pool,
                    commit.id,
                    &e.to_string(),
                    attempt,
                    crate::models::retry_policy::RetryFailureClass::Deterministic,
                )
                .await;
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

        // Load per-configuration policies: each active system's effective policy set
        // (environment + direct assignments), returned as a BTreeMap keyed by
        // NixOS configuration name. Configurations with zero assigned policies
        // produce no entry and are evaluated without policy gates.
        let policies_by_configuration =
            match load_policies_by_configuration_for_eval(pool, flake.id).await {
                Ok(m) => std::sync::Arc::new(m),
                Err(e) => {
                    let failure_class = classify_policy_loader_failure(&e);
                    let e = e.context(format!(
                        "Failed to load per-configuration policies for flake {} (commit {})",
                        flake.id, commit.git_commit_hash,
                    ));
                    error!(
                        "{:#}; refusing evaluation because enforced policy context is unavailable",
                        e
                    );
                    let _ = mark_commit_evaluation_failed(
                        pool,
                        commit.id,
                        &format!("{e:#}"),
                        attempt,
                        failure_class,
                    )
                    .await;
                    return Ok(());
                }
            };

        if let Err(error) = crate::services::composite_enforcement::initialize_eval_passed_attempt(
            pool,
            commit.id,
            attempt,
            &policies_by_configuration,
        )
        .await
        {
            let error = error.context("Failed to initialize evaluation-phase policy evidence");
            return handle_evaluation_attempt_failure(
                pool,
                &cf_state,
                &commit,
                attempt,
                &format!("{error:#}"),
                crate::models::retry_policy::RetryFailureClass::Transient,
            )
            .await;
        }

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

        // Acquire the process-wide commit-evaluation slot before launching
        // nix-eval-jobs.  Only MAX_CONCURRENT_COMMIT_EVALUATIONS bulk evals
        // (plus their fallback phases) may run simultaneously.  This prevents
        // a burst of incoming commits from each spawning their own full-flake
        // Nix evaluation concurrently and exhausting host memory.
        let _eval_permit = match commit_evaluation_limiter().acquire_owned().await {
            Ok(p) => p,
            Err(_) => {
                error!(
                    commit_id = commit.id,
                    "commit evaluation limiter was closed; cannot evaluate"
                );
                return Ok(());
            }
        };
        info!(
            commit_id = commit.id,
            expected_attempt = attempt,
            max_concurrent = MAX_CONCURRENT_COMMIT_EVALUATIONS,
            "commit_evaluation_permit_acquired"
        );

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
                &policies_by_configuration,
                &mock_systems,
                Some(&cf_state),
                Some(&queue_notifier),
            )
            .await
        } else {
            evaluate_with_nix_eval_jobs(
                pool,
                &commit,
                attempt,
                &flake,
                &flake.repo_url,
                &commit.git_commit_hash,
                "all", // Evaluate all systems
                &build_config,
                &server_config,
                &policies_by_configuration,
                Some(&cf_state), // Pass CFState for WebSocket broadcasting
                Some(&queue_notifier),
            )
            .await
        };

        // ── Helper: finalize cancellation side effects ──────────────────
        // Shared by both the success and failure SupersededOrCancelled branches.
        // Inline async closures are not stable, so we use a labelled block.
        //
        // The helper is inlined at each call site below using a macro-style
        // comment so the borrow checker sees each use in its own scope.

        match eval_result {
            Ok(plan) => {
                let results = plan.results.clone();
                let policy_checks = plan.policy_checks.clone();

                info!(
                    commit_id = commit.id,
                    expected_attempt = attempt,
                    "commit_evaluation_finalization_started"
                );
                match finalize_evaluation_attempt(pool, commit.id, attempt, &plan).await {
                    Err(e) => {
                        let error = e.context(format!(
                            "Failed to atomically finalize commit {} evaluation (attempt {})",
                            commit.git_commit_hash, attempt
                        ));
                        let error_text = error.to_string();
                        return handle_evaluation_attempt_failure(
                            pool,
                            &cf_state,
                            &commit,
                            attempt,
                            &error_text,
                            crate::models::retry_policy::RetryFailureClass::Unknown,
                        )
                        .await;
                    }
                    Ok(EvaluationFinalizeOutcome::Cancelled) => {
                        if let Err(err) =
                            crate::queries::commits::cleanup_partial_derivations_for_commit(
                                pool, commit.id,
                            )
                            .await
                        {
                            warn!(
                                "Failed to clean partial derivations for cancelled commit {}: {}",
                                commit.id, err
                            );
                        }
                        crate::handlers::api::commits::broadcast_eval_status(
                            &cf_state,
                            commit.id,
                            "cancelled".to_string(),
                            Some("Evaluation cancelled by user".to_string()),
                        )
                        .await;
                        crate::handlers::api::commits::broadcast_eval_log(
                            &cf_state,
                            commit.id,
                            "🚫 Evaluation cancelled by user".to_string(),
                        )
                        .await;
                        crate::handlers::api::commits::cleanup_eval_channel(&cf_state, commit.id)
                            .await;
                        return Ok(());
                    }
                    Ok(EvaluationFinalizeOutcome::Superseded) => {
                        warn!(
                            "Commit {} evaluation superseded before finalization",
                            commit.git_commit_hash
                        );
                        crate::handlers::api::commits::cleanup_eval_channel(&cf_state, commit.id)
                            .await;
                        return Ok(());
                    }
                    Ok(EvaluationFinalizeOutcome::Completed {
                        derivations,
                        queued_builds,
                    }) => {
                        // Atomic DB finalization succeeded — now safe to run all
                        // external completion side effects.

                        if !queued_builds.is_empty() {
                            info!(
                                "📋 Queued {} build jobs for commit {}, notifying build workers",
                                queued_builds.len(),
                                commit.git_commit_hash
                            );
                            queue_notifier.notify_build_queue();
                            broadcast_queued_builds(&cf_state, commit.id, &queued_builds).await;
                        } else {
                            debug!(
                                "No new build jobs for commit {} (already queued or no ready derivations)",
                                commit.git_commit_hash
                            );
                        }

                        run_post_finalize_derivation_side_effects(pool, &derivations).await;

                        if server_config.auto_hardening_scans {
                            match trigger_commit_hardening_scans(
                                pool.clone(),
                                commit.id,
                                &flake.repo_url,
                                &commit.git_commit_hash,
                            )
                            .await
                            {
                                Ok(count) if count > 0 => {
                                    info!(
                                        "🛡️ Queued {} hardening scans for commit {}",
                                        count, commit.git_commit_hash
                                    );
                                }
                                Ok(_) => {}
                                Err(err) => {
                                    warn!(
                                        "Failed to queue hardening scans for commit {}: {}",
                                        commit.git_commit_hash, err
                                    );
                                }
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

                        for check in policy_checks.iter().filter(|c| !c.meets_requirements) {
                            for warning in &check.warnings {
                                // SECURITY: Evaluator support diagnostics can contain
                                // raw values or source URLs. Sanitize before the first log.
                                let warning =
                                    crate::security::snapshot_redaction::redact_text(warning);
                                warn!("⚠️  {}: {}", check.system_name, warning);
                            }
                        }

                        // Broadcast AFTER the CAS and all persistence are done.
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
                        crate::handlers::api::commits::cleanup_eval_channel(&cf_state, commit.id)
                            .await;
                    }
                }
            }
            Err(e) => {
                // SECURITY: The support error can contain evaluator-controlled
                // values and URLs. Redact before failure handling can log,
                // persist, or broadcast the diagnostic.
                let error_text =
                    crate::security::snapshot_redaction::redact_evaluation_error(&e.to_string());
                return handle_evaluation_attempt_failure(
                    pool,
                    &cf_state,
                    &commit,
                    attempt,
                    &error_text,
                    e.class,
                )
                .await;
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
        EnforcedPolicyLoadSafetyError, builder_stale_timeout_secs, classify_policy_loader_failure,
        evaluation_due_delay, evaluation_policy_digest, new_cve_scan_background_job,
        normalize_custom_policy_expression, parse_deployment_policy_record,
        parse_enforced_policy_record, select_next_pending_commit_id_for_cycle,
    };
    use crate::models::deployment_policies::{
        AssignedPolicy, DeploymentPolicy, DeploymentPolicyRecord, PolicyRule, RuleMode,
    };
    use chrono::Utc;
    use serde_json::json;
    use uuid::Uuid;

    #[tokio::test]
    async fn registered_cve_scan_job_starts_enabled() {
        let (handle, _run_now_rx) = new_cve_scan_background_job(std::time::Duration::from_secs(60));

        let status = handle.status().await;
        assert_eq!(status.id, "cve_scan");
        assert!(
            status.enabled,
            "persisted scan policy should gate an active worker"
        );
    }

    fn policy_record(policy_type: &str, config: serde_json::Value) -> DeploymentPolicyRecord {
        DeploymentPolicyRecord {
            id: Uuid::new_v4(),
            name: format!("{policy_type} policy"),
            description: None,
            policy_type: policy_type.to_string(),
            config,
            enabled: true,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            mapped_requirement_count: 0,
            bundle_usage_count: 0,
        }
    }

    #[test]
    fn enforced_policy_parser_rejects_every_malformed_or_unsupported_type_deterministically() {
        let malformed = [
            ("require_packages", json!({"packages": [42]})),
            ("custom_check", json!({"rules": [false]})),
            ("require_cve_check", json!({"max_critical": "none"})),
            ("time_window", json!({"description": 42})),
            ("require_approvals", json!({"count": "two"})),
            ("canary_rollout", json!({"percentage": "ten"})),
            ("cve_threshold", json!({"thresholds": "none"})),
            ("composite", json!({"schema_version": "one"})),
            ("future_policy_type", json!({})),
        ];

        for (policy_type, config) in malformed {
            let record = policy_record(policy_type, config);
            let error = parse_enforced_policy_record(&record)
                .expect_err("malformed enforced policy must fail closed");
            let error = anyhow::Error::new(error);
            assert_eq!(
                classify_policy_loader_failure(&error),
                crate::models::retry_policy::RetryFailureClass::Deterministic,
                "{policy_type}"
            );
            assert!(error.to_string().contains(policy_type));
        }
    }

    #[test]
    fn enforced_policy_parser_allows_explicit_empty_custom_check() {
        let record = policy_record("custom_check", json!({"mode": "all", "rules": []}));
        assert!(parse_enforced_policy_record(&record).unwrap().is_none());
    }

    #[test]
    fn policy_loader_classifies_semantic_conflicts_as_deterministic() {
        let conflict = anyhow::Error::new(EnforcedPolicyLoadSafetyError(
            "shared configuration has conflicting policy semantics".to_string(),
        ));
        assert_eq!(
            classify_policy_loader_failure(&conflict),
            crate::models::retry_policy::RetryFailureClass::Deterministic
        );
    }

    #[test]
    fn policy_loader_classifies_infrastructure_failures_as_transient() {
        assert_eq!(
            classify_policy_loader_failure(&anyhow::anyhow!("database connection reset")),
            crate::models::retry_policy::RetryFailureClass::Transient
        );
    }

    #[test]
    fn evaluation_due_wakeup_respects_zero_ten_and_thirty_second_backoffs() {
        let now = Utc::now();
        for seconds in [0, 10, 30] {
            assert_eq!(
                evaluation_due_delay(now, now + chrono::Duration::seconds(seconds)),
                std::time::Duration::from_secs(seconds as u64)
            );
        }
        assert_eq!(
            evaluation_due_delay(now, now - chrono::Duration::seconds(1)),
            std::time::Duration::ZERO
        );
    }

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
            mapped_requirement_count: 0,
            bundle_usage_count: 0,
        };

        let parsed = parse_deployment_policy_record(&record).expect("policy should parse");
        match parsed {
            DeploymentPolicy::RequireCrystalForgeAgent { strict } => assert!(strict),
            _ => panic!("expected RequireCrystalForgeAgent variant"),
        }
    }

    #[test]
    fn parse_custom_check_skips_the_explicit_no_enforcement_representation() {
        // The unified policy editor persists "no enforcement" as an explicit
        // empty custom_check rule set. Runtime semantics must agree with that
        // claim: the policy is skipped entirely rather than compiled into an
        // always-true assertion that would silently report a pass.
        let record = DeploymentPolicyRecord {
            id: Uuid::new_v4(),
            name: "no-enforcement".to_string(),
            description: Some("mapped but not enforced".to_string()),
            policy_type: "custom_check".to_string(),
            config: json!({"mode": "all", "rules": []}),
            enabled: true,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            mapped_requirement_count: 3,
            bundle_usage_count: 1,
        };

        assert!(
            parse_deployment_policy_record(&record).is_none(),
            "an empty rule set must not produce an executable policy"
        );
    }

    #[test]
    fn parse_composite_preserves_typed_representation_without_marking_it_nix_executable() {
        let record = DeploymentPolicyRecord {
            id: Uuid::new_v4(),
            name: "mixed".to_string(),
            description: Some("mixed phases".to_string()),
            policy_type: "composite".to_string(),
            config: json!({
                "schema_version": 1,
                "mode": "all",
                "rules": [{
                    "id": "40000000-0000-0000-0000-000000000001",
                    "kind": "cve_block",
                    "config": {"severity": "critical", "max_allowed": 0}
                }]
            }),
            enabled: true,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            mapped_requirement_count: 0,
            bundle_usage_count: 0,
        };

        let parsed = parse_deployment_policy_record(&record).expect("typed representation");
        assert!(matches!(parsed, DeploymentPolicy::Composite { .. }));
        assert!(!parsed.is_nix_evaluated());
    }

    #[test]
    fn parse_require_packages_rejects_an_empty_package_list() {
        let record = DeploymentPolicyRecord {
            id: Uuid::new_v4(),
            name: "packages".to_string(),
            description: Some("required packages".to_string()),
            policy_type: "require_packages".to_string(),
            config: json!({"packages": [], "strict": true}),
            enabled: true,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            mapped_requirement_count: 0,
            bundle_usage_count: 0,
        };

        assert!(parse_deployment_policy_record(&record).is_none());
    }

    #[test]
    fn parse_require_packages_rejects_malformed_entries() {
        let record = DeploymentPolicyRecord {
            id: Uuid::new_v4(),
            name: "packages".to_string(),
            description: Some("required packages".to_string()),
            policy_type: "require_packages".to_string(),
            config: json!({"packages": ["openssh", "", 42], "strict": true}),
            enabled: true,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            mapped_requirement_count: 0,
            bundle_usage_count: 0,
        };

        assert!(parse_deployment_policy_record(&record).is_none());
    }

    #[test]
    fn normalize_custom_policy_expression_rewrites_legacy_config_prefix() {
        let (normalized, changed) =
            normalize_custom_policy_expression("cfg.config.services.auditd.enable or false");
        assert!(changed);
        assert_eq!(normalized, "config.services.auditd.enable or false");
    }

    #[test]
    fn normalize_custom_policy_expression_keeps_cfg_config_prefix() {
        let (normalized, changed) =
            normalize_custom_policy_expression("cfg.config.networking.firewall.enable");
        assert!(changed);
        assert_eq!(normalized, "config.networking.firewall.enable");
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
            mapped_requirement_count: 0,
            bundle_usage_count: 0,
        };

        let parsed = parse_deployment_policy_record(&record).expect("policy should parse");
        match parsed {
            DeploymentPolicy::CustomCheck { expression, .. } => {
                assert_eq!(expression, "config.services.auditd.enable or false")
            }
            _ => panic!("expected CustomCheck variant"),
        }
    }

    #[test]
    fn normalize_custom_policy_expression_preserves_nix_literals_comments_and_boundaries() {
        let expression = r##"cfg.config.services.auditd.enable &&
            "cfg.config.in_a_string" == "cfg.config.foo" # cfg.config.comment
            && mycfg.config.not_a_reference
            && ''cfg.config.in_an_indented_string''
            && /* cfg.config.block_comment */ cfg.config.services.audit.enable
            && "${cfg.config.interpolated}"
            && ''${cfg.config.indented_interpolated}''
            && "\${cfg.config.literal_interpolated}""##;
        let (normalized, changed) = normalize_custom_policy_expression(expression);

        assert!(changed);
        assert!(normalized.contains("config.services.auditd.enable"));
        assert!(normalized.contains("\"cfg.config.in_a_string\""));
        assert!(normalized.contains("# cfg.config.comment"));
        assert!(normalized.contains("mycfg.config.not_a_reference"));
        assert!(normalized.contains("''cfg.config.in_an_indented_string''"));
        assert!(normalized.contains("/* cfg.config.block_comment */"));
        assert!(normalized.contains("config.services.audit.enable"));
        assert!(normalized.contains("\"${config.interpolated}\""));
        assert!(normalized.contains("''${config.indented_interpolated}''"));
        assert!(
            normalized.contains("\"\\${cfg.config.literal_interpolated}\""),
            "normalized={normalized:?}"
        );
    }

    #[test]
    fn normalize_custom_policy_expression_respects_indented_string_escapes() {
        let expression = "''\n  ${cfg.config.real}\n  echo ''${cfg.config.literal}\n  echo ''\\${cfg.config.literal_backslash}\n  echo '''cfg.config.literal_quotes\n''";
        let (normalized, changed) = normalize_custom_policy_expression(expression);

        assert!(changed);
        assert!(normalized.contains("${config.real}"));
        assert!(
            normalized.contains("''${cfg.config.literal}"),
            "normalized={normalized:?}"
        );
        assert!(
            normalized.contains("''\\${cfg.config.literal_backslash}"),
            "normalized={normalized:?}"
        );
        assert!(normalized.contains("'''cfg.config.literal_quotes"));
    }

    #[test]
    fn normalize_custom_policy_expression_ignores_braces_in_nested_interpolation_strings() {
        let expression = r#""${let x = ''}''; in cfg.config.services.auditd.enable}""#;
        let (normalized, changed) = normalize_custom_policy_expression(expression);

        assert!(changed);
        assert_eq!(
            normalized,
            r#""${let x = ''}''; in config.services.auditd.enable}""#
        );
    }

    #[test]
    fn normalize_custom_policy_expression_ignores_braces_in_nested_interpolation_comments() {
        let expression = "\"${let\n  x = 1; # }\nin cfg.config.services.auditd.enable}\"";
        let (normalized, changed) = normalize_custom_policy_expression(expression);

        assert!(changed);
        assert_eq!(
            normalized,
            "\"${let\n  x = 1; # }\nin config.services.auditd.enable}\""
        );
    }

    #[test]
    fn normalize_custom_policy_expression_handles_combined_nested_interpolation_lexing() {
        let expression = r#""${let
  a = ''literal } and ''${notInterpolation}'';
  b = "also }";
  # }
  /* } */
in cfg.config.security.auditd.enable}""#;
        let (normalized, changed) = normalize_custom_policy_expression(expression);

        assert!(changed);
        assert!(normalized.contains("''literal } and ''${notInterpolation}''"));
        assert!(normalized.contains("\"also }\""));
        assert!(normalized.contains("# }"));
        assert!(normalized.contains("/* } */"));
        assert!(normalized.contains("in config.security.auditd.enable}"));
        assert!(!normalized.contains("cfg.config.security.auditd.enable"));
    }

    #[test]
    fn evaluation_policy_digest_is_real_for_an_empty_set() {
        assert_eq!(evaluation_policy_digest(&[]), evaluation_policy_digest(&[]));
        assert_ne!(
            evaluation_policy_digest(&[]),
            evaluation_policy_digest(&[AssignedPolicy {
                policy_id: Uuid::from_u128(1),
                policy_name: "firewall".to_string(),
                policy: DeploymentPolicy::CustomCheck {
                    expression: "cfg.config.networking.firewall.enable".to_string(),
                    description: "firewall enabled".to_string(),
                    field_name: "firewallEnabled".to_string(),
                    strict: true,
                    rules: Vec::new(),
                    mode: RuleMode::All,
                },
            },])
        );
    }

    #[test]
    fn evaluation_policy_digest_changes_when_a_nix_expression_changes() {
        let make_policy = |expression: &str| AssignedPolicy {
            policy_id: Uuid::from_u128(1),
            policy_name: "firewall".to_string(),
            policy: DeploymentPolicy::CustomCheck {
                expression: String::new(),
                description: "firewall".to_string(),
                field_name: String::new(),
                strict: true,
                rules: vec![PolicyRule {
                    expression: expression.to_string(),
                    description: "firewall enabled".to_string(),
                    field_name: "firewallEnabled".to_string(),
                    strict: true,
                }],
                mode: RuleMode::All,
            },
        };

        assert_ne!(
            evaluation_policy_digest(&[make_policy("cfg.config.networking.firewall.enable")]),
            evaluation_policy_digest(&[make_policy(
                "cfg.config.networking.firewall.allowedTCPPorts != []"
            )]),
        );
    }

    #[test]
    fn test_builder_stale_timeout_secs_floor_and_multiplier() {
        assert_eq!(
            builder_stale_timeout_secs(std::time::Duration::from_secs(5)),
            60
        );
        assert_eq!(
            builder_stale_timeout_secs(std::time::Duration::from_secs(20)),
            60
        );
        assert_eq!(
            builder_stale_timeout_secs(std::time::Duration::from_secs(30)),
            90
        );
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
