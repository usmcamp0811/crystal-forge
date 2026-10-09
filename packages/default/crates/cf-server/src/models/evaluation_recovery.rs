//! Controls bounded child phases without changing evaluation or policy semantics.
//!
//! One invocation owns the source, outcomes, deadline and heavy-Nix locks. Each
//! configuration receives at most one isolated CF child. Upstream solo exhaustion
//! is already terminal and MUST NOT receive another isolated attempt.

use std::collections::HashSet;
use tokio::time::{Duration, Instant};

use super::evaluation_pressure::{
    PressureHysteresis, PressureSampler, ResourceSample, SignalReason, StallDecision,
};
use super::evaluation_watchdog::{EvaluationWatchdog, Expiry};

/// Exact upstream 2.35.4 terminal diagnostic, including its numeric budget.
pub(super) fn solo_budget_exhausted(message: &str) -> bool {
    message
        .strip_prefix("evaluation exceeded the memory budget of ")
        .and_then(|s| s.strip_suffix(" MiB (workers * max-memory-size) even when run alone"))
        .is_some_and(|digits| !digits.is_empty() && digits.bytes().all(|c| c.is_ascii_digit()))
}

/// Keeps low-impact silent drops in the existing small fallback boundary.
/// Larger unexplained drops receive bounded isolation, never an OOM claim.
pub(super) fn large_dropout(missing: usize, expected: usize) -> bool {
    missing > super::MAX_INDIVIDUAL_FALLBACKS
        || (missing >= super::MIN_MISSING_FOR_PERCENT_GUARD
            && expected > 0
            && missing as u128 * 100
                > expected as u128 * super::MAX_FALLBACK_MISSING_PERCENT as u128)
}

/// Uses the original capacity snapshot, never live usage or available memory.
pub(super) fn solo_budget(
    plan: &cf_config::evaluator_resources::EvaluatorResourcePlan,
) -> Option<usize> {
    let working = match plan.effective_limit_mb {
        Some(limit) => {
            let reserved = limit.checked_sub(plan.reserve_mb)?;
            let percentage = (limit as u128 * plan.max_memory_percent as u128 / 100) as usize;
            reserved.min(percentage)
        }
        None => 4096,
    };
    Some(working.min(plan.total_budget_mb)).filter(|v| *v > 0)
}

/// Tracks the finite parallel/solo transition graph and isolated identities.
pub(super) struct RecoveryController {
    isolated: HashSet<String>,
    parallel_stalls: u8,
    serial: bool,
    solo_completed: bool,
    cleared: bool,
}

impl RecoveryController {
    pub(super) fn new() -> Self {
        Self {
            isolated: HashSet::new(),
            parallel_stalls: 0,
            serial: false,
            solo_completed: false,
            cleared: false,
        }
    }

    pub(super) fn stalled(&mut self) {
        self.parallel_stalls = self.parallel_stalls.saturating_add(1);
        self.serial = true;
        self.solo_completed = false;
        self.cleared = false;
    }

    pub(super) fn cleared(&mut self) {
        self.cleared = true;
    }

    pub(super) fn completed_solo(&mut self) {
        self.solo_completed = true;
    }

    /// Returns serial work, or restores configured parallelism once only.
    pub(super) fn next(&mut self, remaining: &[String]) -> Option<Vec<String>> {
        if self.parallel_stalls == 1 && self.solo_completed && self.cleared {
            self.serial = false;
            self.solo_completed = false;
            return Some(remaining.to_vec());
        }
        if self.serial {
            let name = remaining
                .iter()
                .find(|name| !self.isolated.contains(*name))?;
            self.isolated.insert(name.clone());
            Some(vec![name.clone()])
        } else {
            Some(remaining.to_vec())
        }
    }

    pub(super) fn serial(&self) -> bool {
        self.serial
    }

    pub(super) fn exclude(&mut self, name: &str) {
        self.isolated.insert(name.to_owned());
    }
}

/// Explains why collection stopped; pressure is not a confirmed Nix error.
#[derive(Clone, Copy, Debug)]
pub(super) enum PhaseStop {
    Expired(Expiry),
    Pressure(SignalReason),
}

/// Shares verified terminal progress independently of stdout-handler awaits.
#[derive(Clone)]
pub(super) struct EvaluationProgress {
    /// Number of validated successes plus confirmed configuration failures.
    pub(super) completed: usize,
    /// Original selected configuration count, unchanged across child phases.
    pub(super) selected_total: usize,
    /// Monotonic receipt of the last verified terminal outcome, or invocation start.
    pub(super) last_completion_at: Instant,
    // SECURITY: Only the setter creates a label. It redacts, removes control
    // characters and caps the retained identifier before any diagnostic reads.
    last_system_label: Option<String>,
}

impl EvaluationProgress {
    /// Starts an invocation's progress baseline with no completed system label.
    pub(super) fn new(selected_total: usize, now: Instant) -> Self {
        Self {
            completed: 0,
            selected_total,
            last_completion_at: now,
            last_system_label: None,
        }
    }

    /// Records terminal progress and a safe bounded label without changing deadlines.
    pub(super) fn record_completion(&mut self, completed: usize, system: &str, now: Instant) {
        self.completed = completed;
        self.last_completion_at = now;
        let label: String = crate::security::snapshot_redaction::redact_text(system)
            .chars()
            .filter(|character| !character.is_control())
            .take(96)
            .collect();
        self.last_system_label = (!label.is_empty()).then_some(label);
    }

    fn remaining(&self) -> usize {
        self.selected_total.saturating_sub(self.completed)
    }
}

fn assessment_labels(decision: StallDecision) -> (&'static str, Option<&'static str>) {
    match decision {
        StallDecision::Unknown => ("unknown", None),
        StallDecision::Observe => ("observe", None),
        StallDecision::Cleared => ("clear", None),
        StallDecision::Recover(SignalReason::ReclaimPressure) => {
            ("recover", Some("reclaim_pressure"))
        }
        StallDecision::Recover(SignalReason::BlockedIo) => ("recover", Some("blocked_io")),
        StallDecision::Recover(SignalReason::WorkerReplacement) => {
            ("recover", Some("worker_replacement"))
        }
    }
}

fn active_cpu(sample: &ResourceSample, evaluator_pid: u32) -> bool {
    sample.complete
        && !sample.processes.is_empty()
        && sample
            .processes
            .iter()
            .all(|p| p.state != 'D' && p.cpu_delta_ticks.is_some())
        && sample.processes.iter().any(|p| {
            p.key.pid != evaluator_pid
                && p.parent_pid == evaluator_pid
                && p.cpu_delta_ticks.is_some_and(|v| v > 0)
        })
}

/// Samples independently of pending stdout-handler awaits.
///
/// Idle expiry is an assessment checkpoint. Direct-worker CPU evidence can
/// waive silence until the next sample, but cannot advance output activity or
/// the shared overall deadline. Unknown diagnostics never imply healthy work.
pub(super) async fn monitor(
    watchdog: &EvaluationWatchdog,
    evaluator_pid: u32,
    sampler: &mut PressureSampler,
    hysteresis: &mut PressureHysteresis,
    mut progress: tokio::sync::watch::Receiver<EvaluationProgress>,
    cleared: &mut bool,
) -> PhaseStop {
    let mut ticker = tokio::time::interval(Duration::from_secs(30));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut latest = None;
    let mut idle_waived = false;
    loop {
        tokio::select! {
            biased;
            _ = tokio::time::sleep_until(watchdog.overall_deadline()) => {
                return PhaseStop::Expired(Expiry::Overall);
            }
            _ = ticker.tick() => {
                let sample = sampler.sample(Instant::now());
                let progress = progress.borrow_and_update().clone();
                let last_completion_elapsed = sample.now.saturating_duration_since(progress.last_completion_at);
                let decision = hysteresis.update(&sample, progress.completed, last_completion_elapsed);
                let (assessment, reason) = assessment_labels(decision);
                tracing::info!(pid = evaluator_pid, members = sample.processes.len(),
                    completed = progress.completed, selected = progress.selected_total,
                    remaining = progress.remaining(), last_completion_secs = last_completion_elapsed.as_secs(),
                    output_idle_secs = watchdog.real_output_elapsed(sample.now).as_secs(),
                    last_system = progress.last_system_label.as_deref(), assessment, reason,
                    "Evaluator progress assessment");
                sample.log();
                match decision {
                    StallDecision::Recover(reason) => return PhaseStop::Pressure(reason),
                    StallDecision::Cleared => *cleared = true,
                    StallDecision::Observe | StallDecision::Unknown => {}
                }
                if idle_waived && !active_cpu(&sample, evaluator_pid) {
                    return PhaseStop::Expired(Expiry::Idle);
                }
                latest = Some(sample);
            }
            expiry = watchdog.wait(), if !idle_waived => {
                match expiry {
                    Expiry::Overall => return PhaseStop::Expired(expiry),
                    Expiry::Idle => {
                        if latest.as_ref().is_some_and(|s: &ResourceSample| {
                            Instant::now().saturating_duration_since(s.now) <= Duration::from_secs(60)
                                && active_cpu(s, evaluator_pid)
                        }) {
                            idle_waived = true;
                        } else {
                            return PhaseStop::Expired(expiry);
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_labels_are_redacted_bounded_and_do_not_extend_deadlines() {
        let start = Instant::now();
        let watchdog = EvaluationWatchdog::with_deadline(
            start,
            Duration::from_secs(10),
            start + Duration::from_secs(60),
        )
        .unwrap();
        let mut progress = EvaluationProgress::new(8, start);
        assert_eq!((progress.completed, progress.remaining()), (0, 8));
        assert!(progress.last_system_label.is_none());
        let label = format!(
            "https://private-user:private-token@example.test/{}\n\u{1b}",
            "x".repeat(200)
        );
        progress.record_completion(3, &label, start + Duration::from_secs(30));
        let safe = progress.last_system_label.as_ref().unwrap();
        assert!(safe.chars().count() <= 96);
        assert!(safe.chars().all(|character| !character.is_control()));
        assert!(!safe.contains("private-token"));
        assert_eq!(
            (
                progress.completed,
                progress.selected_total,
                progress.remaining()
            ),
            (3, 8, 5)
        );
        assert_eq!(
            watchdog.idle_elapsed(start + Duration::from_secs(30)),
            Duration::from_secs(30)
        );
        assert_eq!(watchdog.overall_deadline(), start + Duration::from_secs(60));
    }

    #[test]
    fn assessments_have_only_finite_operator_labels() {
        assert_eq!(assessment_labels(StallDecision::Unknown), ("unknown", None));
        assert_eq!(assessment_labels(StallDecision::Observe), ("observe", None));
        assert_eq!(assessment_labels(StallDecision::Cleared), ("clear", None));
        assert_eq!(
            assessment_labels(StallDecision::Recover(SignalReason::BlockedIo)),
            ("recover", Some("blocked_io"))
        );
        assert_eq!(
            assessment_labels(StallDecision::Recover(SignalReason::ReclaimPressure)),
            ("recover", Some("reclaim_pressure"))
        );
        assert_eq!(
            assessment_labels(StallDecision::Recover(SignalReason::WorkerReplacement)),
            ("recover", Some("worker_replacement"))
        );
    }

    #[test]
    fn upstream_solo_message_is_exact_not_generic_memory_or_signal_text() {
        assert!(solo_budget_exhausted(
            "evaluation exceeded the memory budget of 4096 MiB (workers * max-memory-size) even when run alone"
        ));
        for message in [
            "evaluation exceeded the memory budget of -1 MiB (workers * max-memory-size) even when run alone",
            "out of memory",
            "worker was killed by signal 9",
            "evaluation exceeded the memory budget of  MiB (workers * max-memory-size) even when run alone",
        ] {
            assert!(!solo_budget_exhausted(message));
        }
    }

    #[test]
    fn solo_budget_caps_fixed_override_at_original_working_boundary() {
        use cf_config::evaluator_resources::{EvaluatorResourceMode, EvaluatorResourcePlan};
        let mut plan = EvaluatorResourcePlan {
            requested_workers: 2,
            effective_workers: 2,
            physical_memory_mb: Some(32768),
            cgroup_memory_mb: Some(16384),
            cgroup_memory_high_mb: Some(8192),
            effective_limit_mb: Some(8192),
            reserve_mb: 2048,
            max_memory_percent: 75,
            total_budget_mb: 16000,
            per_worker_mb: 8000,
            mode: EvaluatorResourceMode::Fixed,
        };
        assert_eq!(solo_budget(&plan), Some(6144));
        assert_eq!(
            plan.per_worker_mb, 8000,
            "initial explicit override stays exact"
        );
        plan.total_budget_mb = 4096;
        assert_eq!(solo_budget(&plan), Some(4096));
        plan.effective_limit_mb = Some(1024);
        assert_eq!(solo_budget(&plan), None);
        plan.effective_limit_mb = None;
        assert_eq!(solo_budget(&plan), Some(4096));
    }

    #[test]
    fn isolated_limit_is_per_configuration_not_four_missing_systems() {
        let names: Vec<String> = (0..12).map(|i| format!("configuration-{i}")).collect();
        let mut controller = RecoveryController::new();
        controller.stalled();
        controller.exclude(&names[0]); // upstream exhausted solo itself
        for name in &names[1..] {
            assert_eq!(controller.next(&names), Some(vec![name.clone()]));
        }
        assert_eq!(controller.next(&names), None);
    }

    #[test]
    fn dropout_boundary_preserves_small_fallback_limits() {
        assert!(!large_dropout(1, 3));
        assert!(!large_dropout(4, 16));
        assert!(large_dropout(5, 100));
        assert!(large_dropout(2, 7));
        assert!(!large_dropout(2, 8));
    }

    #[test]
    fn idle_cpu_evidence_requires_complete_known_nonblocked_processes() {
        use super::super::evaluation_pressure::{ProcessKey, ProcessSample};
        let mut sample = ResourceSample {
            now: Instant::now(),
            interval: Some(Duration::from_secs(30)),
            complete: true,
            ticks_per_second: Some(100),
            cgroups: Vec::new(),
            worker_replacements: Some(0),
            processes: vec![ProcessSample {
                key: ProcessKey {
                    pid: 12,
                    start_ticks: 34,
                },
                parent_pid: 11,
                state: 'R',
                cpu_ticks: 300,
                cpu_delta_ticks: Some(300),
                rss_bytes: 1024,
            }],
        };
        assert!(active_cpu(&sample, 11));
        assert!(
            !active_cpu(&sample, 12),
            "leader-only CPU cannot prove worker progress"
        );
        sample.complete = false;
        assert!(!active_cpu(&sample, 11));
        sample.complete = true;
        sample.processes[0].state = 'D';
        assert!(!active_cpu(&sample, 11));
        sample.processes[0].state = 'R';
        sample.processes[0].cpu_delta_ticks = None;
        assert!(!active_cpu(&sample, 11));
    }

    #[test]
    fn helper_cpu_cannot_waive_idle_when_direct_workers_make_no_progress() {
        use super::super::evaluation_pressure::{ProcessKey, ProcessSample};
        let worker = ProcessSample {
            key: ProcessKey {
                pid: 12,
                start_ticks: 1,
            },
            parent_pid: 11,
            state: 'S',
            cpu_ticks: 100,
            cpu_delta_ticks: Some(0),
            rss_bytes: 4096,
        };
        let helper = ProcessSample {
            key: ProcessKey {
                pid: 13,
                start_ticks: 2,
            },
            parent_pid: 12,
            state: 'R',
            cpu_ticks: 1000,
            cpu_delta_ticks: Some(900),
            rss_bytes: 4096,
        };
        let mut sample = ResourceSample {
            now: Instant::now(),
            interval: Some(Duration::from_secs(30)),
            complete: true,
            ticks_per_second: Some(100),
            cgroups: Vec::new(),
            worker_replacements: Some(0),
            processes: vec![worker, helper],
        };
        assert!(!active_cpu(&sample, 11));
        sample.processes[0].cpu_delta_ticks = Some(1);
        assert!(active_cpu(&sample, 11));
    }

    #[test]
    fn second_stall_is_sticky_serial_and_each_identity_is_isolated_once() {
        let names = vec!["a".into(), "b".into(), "c".into()];
        let mut recovery = RecoveryController::new();
        recovery.stalled();
        assert_eq!(recovery.next(&names), Some(vec!["a".into()]));
        recovery.completed_solo();
        assert_eq!(recovery.next(&names[1..]), Some(vec!["b".into()]));
        recovery.cleared();
        assert_eq!(recovery.next(&names[2..]), Some(vec!["c".into()]));
        assert!(!recovery.serial());
        recovery.stalled();
        recovery.completed_solo();
        recovery.cleared();
        assert_eq!(recovery.next(&names), Some(vec!["c".into()]));
        assert!(recovery.serial());
        assert_eq!(recovery.next(&names), None);
    }
}
