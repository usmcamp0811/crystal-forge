//! Tracks bulk evaluator silence independently of its spawn-anchored deadline.
//!
//! Output can extend only the idle deadline. Overall expiry takes precedence
//! when both deadlines are due. Process ownership and cleanup stay with the
//! caller's process-group guard.

use tokio::time::{Duration, Instant};

/// Cadence for eligible cooperative cancellation polls, in seconds.
/// Handler awaits can delay the actual query at the next loop boundary.
/// This cadence is independent of idle and overall deadline enforcement.
const CANCELLATION_POLL_INTERVAL: Duration = Duration::from_secs(2);

/// Starts cancellation polling after the first two-second interval.
///
/// Consumes Tokio's immediate initial tick so production and tests share the
/// same cancellation cadence without an extra database query at startup.
pub(super) async fn cancellation_ticker() -> tokio::time::Interval {
    let mut ticker = tokio::time::interval(CANCELLATION_POLL_INTERVAL);
    ticker.tick().await;
    ticker
}

/// Bounds every collection await with independent idle and overall timers.
///
/// The timer is polled first when both branches are ready. Thus output cannot
/// win against an already expired deadline. Activity updates wake the monitor
/// while collection is inside a handler or log flush. Collection is dropped
/// before this function returns; the caller then kills and reaps outside this
/// race. Collection MUST NOT terminate or disarm the caller's process guard.
///
/// # Errors
/// Returns the expired deadline, with overall precedence when both are due.
#[cfg(test)]
pub(super) async fn within_deadlines<F: std::future::Future>(
    watchdog: &EvaluationWatchdog,
    collection: F,
) -> Result<F::Output, Expiry> {
    tokio::select! {
        biased;
        expiry = watchdog.wait() => Err(expiry),
        output = collection => Ok(output),
    }
}

/// Identifies the pipe that supplied a complete output line.
#[derive(Clone, Copy)]
pub(super) enum OutputStream {
    Stdout,
    Stderr,
}

/// Identifies the deadline that requires process-group termination.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Expiry {
    Overall,
    Idle,
}

/// Shares monotonic output activity with an independent deadline monitor.
///
/// Only actual nonblank line receipt can advance activity. An expired deadline
/// is terminal: buffered output cannot revive the evaluator. Deadline activity
/// and actual receipt diagnostics each store one timestamp, without retaining
/// output or allocating per line. Diagnostic receipt never drives expiry.
pub(super) struct EvaluationWatchdog {
    overall_deadline: Instant,
    last_output: tokio::sync::watch::Sender<Instant>,
    // Diagnostics retain actual line receipt even after idle expiry. This
    // timestamp MUST NOT participate in deadline or recovery decisions.
    last_received_output: tokio::sync::watch::Sender<Instant>,
    idle_timeout: Duration,
}

impl EvaluationWatchdog {
    /// Starts both clocks at spawn using positive durations.
    ///
    /// # Errors
    /// Returns a static error for zero durations or unrepresentable deadlines.
    pub(super) fn new(
        spawned_at: Instant,
        idle: Duration,
        overall: Duration,
    ) -> anyhow::Result<Self> {
        anyhow::ensure!(
            !idle.is_zero() && !overall.is_zero(),
            "evaluator timeouts must be positive"
        );
        Ok(Self {
            overall_deadline: spawned_at
                .checked_add(overall)
                .ok_or_else(|| anyhow::anyhow!("evaluator overall deadline is out of range"))?,
            last_output: {
                anyhow::ensure!(
                    spawned_at.checked_add(idle).is_some(),
                    "evaluator idle deadline is out of range"
                );
                tokio::sync::watch::channel(spawned_at).0
            },
            last_received_output: tokio::sync::watch::channel(spawned_at).0,
            idle_timeout: idle,
        })
    }

    /// Uses the invocation ceiling for every replacement child and cleanup.
    ///
    /// # Errors
    /// Returns an error for a zero idle duration or an expired shared ceiling.
    pub(super) fn with_deadline(
        spawned_at: Instant,
        idle: Duration,
        overall_deadline: Instant,
    ) -> anyhow::Result<Self> {
        let remaining = overall_deadline
            .checked_duration_since(spawned_at)
            .filter(|duration| !duration.is_zero())
            .ok_or_else(|| anyhow::anyhow!("evaluation invocation deadline expired"))?;
        let mut watchdog = Self::new(spawned_at, idle, remaining)?;
        watchdog.overall_deadline = overall_deadline;
        Ok(watchdog)
    }

    /// Advances activity for a timely nonblank line, never an expired deadline.
    pub(super) fn record_output(&self, stream: OutputStream, line: &str, now: Instant) {
        match stream {
            OutputStream::Stdout | OutputStream::Stderr if !line.trim().is_empty() => {
                self.last_received_output.send_if_modified(|last| {
                    if now < *last {
                        false
                    } else {
                        *last = now;
                        true
                    }
                });
                // CONCURRENCY: Check expiry and advance under the same watch
                // write lock. No late or out-of-order observation can revive
                // an expired idle deadline, even before the monitor wakes.
                self.last_output.send_if_modified(|last_output| {
                    if now < *last_output || self.expired_since(*last_output, now).is_some() {
                        false
                    } else {
                        *last_output = now;
                        true
                    }
                });
            }
            _ => {}
        }
    }

    /// Returns the absolute ceiling shared by streaming and final child wait.
    pub(super) fn overall_deadline(&self) -> Instant {
        self.overall_deadline
    }

    /// Returns elapsed silence for diagnostics, in monotonic time.
    pub(super) fn idle_elapsed(&self, now: Instant) -> Duration {
        now.saturating_duration_since(*self.last_output.borrow())
    }

    /// Returns diagnostic age since the last actual nonblank received line.
    /// Late lines update this age without reviving an expired idle deadline.
    pub(super) fn real_output_elapsed(&self, now: Instant) -> Duration {
        now.saturating_duration_since(*self.last_received_output.borrow())
    }

    /// Returns overall expiry first, including after recent output.
    pub(super) fn expired(&self, now: Instant) -> Option<Expiry> {
        self.expired_since(*self.last_output.borrow(), now)
    }

    fn idle_deadline(&self, last_output: Instant) -> Instant {
        last_output
            .checked_add(self.idle_timeout)
            .unwrap_or(self.overall_deadline())
            .min(self.overall_deadline())
    }

    fn expired_since(&self, last_output: Instant, now: Instant) -> Option<Expiry> {
        if now >= self.overall_deadline {
            Some(Expiry::Overall)
        } else if now >= self.idle_deadline(last_output) {
            Some(Expiry::Idle)
        } else {
            None
        }
    }

    /// Sleeps until the exact next deadline rather than a diagnostic heartbeat.
    pub(super) async fn wait(&self) -> Expiry {
        let mut activity = self.last_output.subscribe();
        loop {
            let last_output = *activity.borrow_and_update();
            if let Some(expiry) = self.expired(Instant::now()) {
                return expiry;
            }
            tokio::select! {
                biased;
                _ = tokio::time::sleep_until(self.idle_deadline(last_output)) => {
                    // A timely activity update may have advanced the idle
                    // deadline before this timer was polled. Re-read it.
                }
                _ = activity.changed() => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn late_real_output_updates_diagnostics_without_reviving_expired_idle() {
        let start = Instant::now();
        let watchdog =
            EvaluationWatchdog::new(start, Duration::from_secs(10), Duration::from_secs(60))
                .unwrap();
        watchdog.record_output(
            OutputStream::Stdout,
            "late actual output",
            start + Duration::from_secs(15),
        );
        assert_eq!(
            watchdog.expired(start + Duration::from_secs(16)),
            Some(Expiry::Idle)
        );
        assert_eq!(
            watchdog.idle_elapsed(start + Duration::from_secs(16)),
            Duration::from_secs(16)
        );
        assert_eq!(
            watchdog.real_output_elapsed(start + Duration::from_secs(16)),
            Duration::from_secs(1)
        );
        watchdog.record_output(OutputStream::Stderr, " \t", start + Duration::from_secs(17));
        assert_eq!(
            watchdog.real_output_elapsed(start + Duration::from_secs(18)),
            Duration::from_secs(3)
        );
        assert_eq!(watchdog.overall_deadline(), start + Duration::from_secs(60));
    }

    #[test]
    fn replacement_child_cannot_reset_shared_invocation_deadline() {
        let first_spawn = Instant::now();
        let deadline = first_spawn + Duration::from_secs(100);
        let replacement = EvaluationWatchdog::with_deadline(
            first_spawn + Duration::from_secs(90),
            Duration::from_secs(30),
            deadline,
        )
        .unwrap();
        assert_eq!(replacement.overall_deadline(), deadline);
        assert_eq!(replacement.expired(deadline), Some(Expiry::Overall));
        assert!(
            EvaluationWatchdog::with_deadline(deadline, Duration::from_secs(30), deadline).is_err()
        );
    }

    #[test]
    fn idle_301_survives_default_900_and_expires_at_threshold() {
        let spawn = Instant::now();
        let watchdog =
            EvaluationWatchdog::new(spawn, Duration::from_secs(900), Duration::from_secs(3600))
                .unwrap();
        assert_eq!(watchdog.expired(spawn + Duration::from_secs(301)), None);
        assert_eq!(watchdog.expired(spawn + Duration::from_secs(899)), None);
        assert_eq!(
            watchdog.expired(spawn + Duration::from_secs(900)),
            Some(Expiry::Idle)
        );
        assert_eq!(
            watchdog.expired(spawn + Duration::from_secs(901)),
            Some(Expiry::Idle)
        );
    }

    #[test]
    fn stdout_and_stderr_reset_silence_but_not_overall() {
        let spawn = Instant::now();
        for stream in [OutputStream::Stdout, OutputStream::Stderr] {
            let watchdog =
                EvaluationWatchdog::new(spawn, Duration::from_secs(10), Duration::from_secs(30))
                    .unwrap();
            watchdog.record_output(stream, "output", spawn + Duration::from_secs(9));
            assert_eq!(watchdog.expired(spawn + Duration::from_secs(10)), None);
            watchdog.record_output(stream, " \t", spawn + Duration::from_secs(18));
            assert_eq!(
                watchdog.expired(spawn + Duration::from_secs(19)),
                Some(Expiry::Idle)
            );
            let watchdog =
                EvaluationWatchdog::new(spawn, Duration::from_secs(10), Duration::from_secs(30))
                    .unwrap();
            for seconds in [9, 18, 27, 29] {
                watchdog.record_output(stream, "recent", spawn + Duration::from_secs(seconds));
            }
            assert_eq!(
                watchdog.expired(spawn + Duration::from_secs(30)),
                Some(Expiry::Overall)
            );
            assert_eq!(watchdog.overall_deadline(), spawn + Duration::from_secs(30));
        }
    }

    #[test]
    fn overall_precedes_idle_when_both_expire() {
        let spawn = Instant::now();
        let watchdog =
            EvaluationWatchdog::new(spawn, Duration::from_secs(10), Duration::from_secs(10))
                .unwrap();
        assert_eq!(
            watchdog.expired(spawn + Duration::from_secs(10)),
            Some(Expiry::Overall)
        );
    }

    #[tokio::test]
    async fn exact_idle_timer_does_not_wait_for_heartbeat() {
        let watchdog = EvaluationWatchdog::new(
            Instant::now(),
            Duration::from_millis(20),
            Duration::from_secs(2),
        )
        .unwrap();
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(1), watchdog.wait())
                .await
                .unwrap(),
            Expiry::Idle
        );
    }

    #[tokio::test]
    async fn cancellation_is_prompt_while_watchdog_waits() {
        let watchdog = EvaluationWatchdog::new(
            Instant::now(),
            Duration::from_secs(900),
            Duration::from_secs(3600),
        )
        .unwrap();
        let (cancel, cancelled) = tokio::sync::oneshot::channel();
        cancel.send(()).unwrap();
        tokio::time::timeout(Duration::from_millis(100), async {
            tokio::select! {
                _ = cancelled => {},
                expiry = watchdog.wait() => panic!("unexpected expiry: {expiry:?}"),
            }
        })
        .await
        .expect("cancellation must not wait for an output or watchdog deadline");
    }

    #[tokio::test]
    async fn cancellation_poll_fires_before_long_watchdog_deadlines() {
        let watchdog = EvaluationWatchdog::new(
            Instant::now(),
            Duration::from_secs(900),
            Duration::from_secs(3600),
        )
        .unwrap();
        let mut ticker = cancellation_ticker().await;
        tokio::time::timeout(Duration::from_secs(3), async {
            tokio::select! {
                _ = ticker.tick() => {},
                expiry = watchdog.wait() => panic!("unexpected expiry: {expiry:?}"),
            }
        })
        .await
        .expect("cancellation poll must occur at two seconds despite silent output");
    }

    #[test]
    fn rejects_zero_and_unrepresentable_deadlines() {
        let spawn = Instant::now();
        assert!(EvaluationWatchdog::new(spawn, Duration::ZERO, Duration::from_secs(1)).is_err());
        assert!(EvaluationWatchdog::new(spawn, Duration::from_secs(1), Duration::MAX).is_err());
    }

    #[tokio::test]
    async fn expired_overall_precedes_ready_output_handler() {
        let watchdog = EvaluationWatchdog::new(
            Instant::now(),
            Duration::from_secs(1),
            Duration::from_millis(1),
        )
        .unwrap();
        tokio::time::sleep(Duration::from_millis(5)).await;
        assert_eq!(
            within_deadlines(&watchdog, std::future::ready(())).await,
            Err(Expiry::Overall)
        );
    }

    #[test]
    fn late_or_out_of_order_output_cannot_revive_idle_deadline() {
        let spawn = Instant::now();
        for stream in [OutputStream::Stdout, OutputStream::Stderr] {
            let watchdog =
                EvaluationWatchdog::new(spawn, Duration::from_secs(600), Duration::from_secs(3600))
                    .unwrap();
            watchdog.record_output(
                stream,
                "buffered late line",
                spawn + Duration::from_secs(601),
            );
            assert_eq!(
                watchdog.expired(spawn + Duration::from_secs(601)),
                Some(Expiry::Idle)
            );
            let watchdog =
                EvaluationWatchdog::new(spawn, Duration::from_secs(10), Duration::from_secs(30))
                    .unwrap();
            watchdog.record_output(stream, "current", spawn + Duration::from_secs(9));
            watchdog.record_output(stream, "old", spawn + Duration::from_secs(2));
            assert_eq!(
                watchdog.idle_elapsed(spawn + Duration::from_secs(10)),
                Duration::from_secs(1)
            );
        }
    }

    #[tokio::test]
    async fn expired_idle_precedes_buffered_output_at_601_seconds() {
        let spawn = Instant::now() - Duration::from_secs(601);
        let watchdog =
            EvaluationWatchdog::new(spawn, Duration::from_secs(600), Duration::from_secs(3600))
                .unwrap();
        let mut consumed = false;
        let result = within_deadlines(&watchdog, async {
            consumed = true;
            watchdog.record_output(OutputStream::Stdout, "buffered", Instant::now());
        })
        .await;
        assert_eq!(result, Err(Expiry::Idle));
        assert!(
            !consumed,
            "expired idle must win before a ready buffered line is consumed"
        );
    }

    #[tokio::test]
    async fn outer_idle_expires_during_pending_output_handler_or_log_flush() {
        for stream in [OutputStream::Stdout, OutputStream::Stderr] {
            let watchdog = EvaluationWatchdog::new(
                Instant::now(),
                Duration::from_millis(30),
                Duration::from_secs(1),
            )
            .unwrap();
            let mut entered_handler = false;
            let result = tokio::time::timeout(
                Duration::from_millis(250),
                within_deadlines(&watchdog, async {
                    watchdog.record_output(stream, "received line", Instant::now());
                    entered_handler = true;
                    std::future::pending::<()>().await;
                }),
            )
            .await
            .expect("idle monitor must run while output handling or persistence is pending");
            assert!(entered_handler);
            assert_eq!(result, Err(Expiry::Idle));
        }
    }

    #[tokio::test]
    async fn activity_notifications_rearm_outer_idle_but_not_overall() {
        let watchdog = EvaluationWatchdog::new(
            Instant::now(),
            Duration::from_millis(100),
            Duration::from_millis(250),
        )
        .unwrap();
        let mut count = 0;
        let result = within_deadlines(&watchdog, async {
            loop {
                tokio::time::sleep(Duration::from_millis(20)).await;
                let stream = if count % 2 == 0 {
                    OutputStream::Stdout
                } else {
                    OutputStream::Stderr
                };
                watchdog.record_output(stream, "recent", Instant::now());
                count += 1;
            }
        })
        .await;
        assert!(count >= 2, "both output streams must notify the monitor");
        assert_eq!(result, Err::<(), _>(Expiry::Overall));
    }
}
