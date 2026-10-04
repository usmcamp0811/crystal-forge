---
type: Architecture
title: "Event-Driven Queue Architecture"
description: "Describes the QueueNotifier bounded MPSC wakeup channels, the eval queue and build queue flows, notification guarantees, and fallback polling; open it when changing queue wakeups or worker loops."
tags:
  - crystal-forge
  - architecture
  - queue
  - eval
  - build
implementation_status: partial
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-03T22:57:15-05:00
sources:
  - id: origin
    resource: "Crystal Forge repository file docs/architecture.md at commit 3b23d36f"
    title: "ADR-000: Crystal Forge Architecture Overview"
  - id: code-1
    resource: "Crystal Forge repository file packages/default/crates/cf-server/src/queue/mod.rs at commit 3b23d36f"
    title: QueueNotifier implementation
  - id: code-2
    resource: "Crystal Forge repository file packages/default/crates/cf-server/src/server/mod.rs at commit 3b23d36f"
    title: Background loops and evaluation loop
  - id: code-3
    resource: "Crystal Forge repository file packages/default/crates/cf-server/src/handlers/webhook.rs at commit 3b23d36f"
    title: Webhook handler
  - id: code-4
    resource: "Crystal Forge repository file packages/default/crates/cf-config/src/config/flakes.rs at commit 3b23d36f"
    title: Flake loop interval defaults
  - id: code-5
    resource: "Crystal Forge repository file packages/default/crates/cf-builder/src/bin/builder.rs at commit 3b23d36f"
    title: Builder job polling loop
verified:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-04T08:50:00-05:00
---

# Event-Driven Queue Architecture

> **Status:** partial. `QueueNotifier` exists with capacity-1 channels (`packages/default/crates/cf-server/src/queue/mod.rs`). The eval wakeup is implemented: `run_commit_evaluation_loop` waits on `wait_for_eval_work()` together with a fallback ticker. `notify_build_queue()` is called after build jobs are queued and during recovery, but `wait_for_build_work()` has no caller outside `queue/mod.rs`. Builders are separate API-only processes, so the build wakeup remains "NOT YET IMPLEMENTED" as described below. The webhook handler does not notify the eval queue (see Migration verification notes).

Crystal Forge uses an event-driven architecture for both evaluation and build queues, replacing polling-based approaches with immediate notifications.

## Queue Notification System

The `QueueNotifier` provides bounded event channels using Tokio MPSC:

```rust
pub struct QueueNotifier {
    eval_tx: mpsc::Sender<()>,   // channel(1), coalesced wakeups
    eval_rx: Arc<Mutex<mpsc::Receiver<()>>>,
    build_tx: mpsc::Sender<()>,  // channel(1), coalesced wakeups
    build_rx: Arc<Mutex<mpsc::Receiver<()>>>,
}
```

**Key Benefits**:
- **Zero-latency triggering**: Work starts immediately when commits/jobs arrive
- **Bounded memory**: channel capacity is 1 and duplicate wakeups are coalesced
- **Idle efficiency**: No CPU cycles wasted polling empty queues
- **Fallback safety**: Periodic ticks catch any missed notifications

## Eval Queue Flow

```
Commit Insert → notify_eval_queue() → Eval Loop Wakes → Process Pending
                                    ↓
                        (fallback: 60s ticker)
```

**Trigger Points**:
1. Flake polling discovers new commits
2. Webhook receives push notification
3. Manual commit insertion via API

> **Status:** Trigger points 1 and 3 call `notify_eval_queue()` (`run_flake_polling_loop` in `server/mod.rs`; `handlers/api/commits.rs` and `handlers/api/flakes.rs`). Trigger point 2 is incomplete: `webhook_handler` in `handlers/webhook.rs` inserts the commit but does not notify the queue, so a webhook commit waits for the next fallback tick.

**Processing Loop**:
```rust
loop {
    process_pending_commits(&pool, &cf_state, &queue_notifier).await;

    tokio::select! {
        _ = ticker.tick() => { /* fallback: every 60s */ }
        _ = queue_notifier.wait_for_eval_work() => { /* immediate */ }
    }
}
```

## Build Queue Flow

```
Eval Complete → create_build_jobs() → notify_build_queue() → Build Workers Wake
                                                           ↓
                                        (NOT YET IMPLEMENTED: workers still poll 5s)
```

**Current State**:
- Server-side build job creation triggers notification
- Build workers (separate processes) still poll every 5s
- Future: PostgreSQL LISTEN/NOTIFY or unified process model

## Notification Guarantees

**Fire-and-Forget Semantics**:
- Notifications never block the sender
- Dropped receivers (server shutdown) are silently ignored
- Multiple notifications coalesce into one pending wakeup

**Ordering Scope**:
- MPSC channels preserve send order for server-internal wakeup delivery.
- Global work claiming across separate worker processes remains database/poll driven and is not a strict cross-process FIFO guarantee.

**Fallback Polling**:
- Eval loop: 60s ticker (catches DB corruption, missed signals). The interval is `flakes.commit_evaluation_interval` (default 60s). The loop also wakes at the next durable retry time of a delayed evaluation attempt.
- Build workers: 5s ticker (until event-driven build implemented). The interval is `builder.poll_interval` (default 5s) in each API-only builder process.

## Related concepts

- [Core components](../components/core-components.md) - server and builder processes involved
- [Evaluation and build queue pipeline](../workflows/evaluation-and-build-queue-pipeline.md) - database-side behavior of both queues
- [Derivation processing loops](derivation-processing-loops.md) - the loops these notifications wake

## Migration verification notes

Scope: all behavioral claims of this concept were compared with the code. Verified: channel construction, capacity, coalescing, fire-and-forget semantics, eval loop select, fallback interval, build-queue wakeup gap, builder poll interval.

- Claim: `QueueNotifier` has capacity-1 channels with coalesced, fire-and-forget wakeups.
  Finding: `mpsc::channel(1)` for both queues; `try_send` handles `Full` and `Closed` without error.
  Evidence: `packages/default/crates/cf-server/src/queue/mod.rs` (`QueueNotifier::new`, `notify_eval_queue`, `notify_build_queue`).
  Case: implemented.
- Claim: The eval loop selects on the notifier and a 60s fallback ticker.
  Finding: `run_commit_evaluation_loop` also processes pending work before waiting, and adds a third branch that wakes at the next evaluation `available_at` time. Ticker period is `flakes.commit_evaluation_interval`, default 60s.
  Evidence: `cf-server/src/server/mod.rs` (`run_commit_evaluation_loop`); `cf-config/src/config/flakes.rs`.
  Case: documentation stale (third wake source added; text extended in place).
- Claim: The webhook is a trigger point for `notify_eval_queue()`.
  Finding: `webhook_handler` takes only the database pool, inserts the commit, and never calls the notifier. The rustdoc of `notify_eval_queue` also lists the webhook, which is stale relative to the handler.
  Evidence: `cf-server/src/handlers/webhook.rs`; `cf-server/src/queue/mod.rs`.
  Case: actual implementation defect (design intent: webhook wakes the eval loop). Listed for a backlog task.
- Claim: Build workers do not yet wait on `notify_build_queue()` and poll every 5s.
  Finding: Still true. `wait_for_build_work()` has no caller outside tests; builders poll `GET/POST /api/v1/builders/:id/next-job` at `builder.poll_interval` (default 5s).
  Evidence: `cf-server/src/queue/mod.rs`; `cf-builder/src/bin/builder.rs` (`run_api_job_loop`); `cf-config/src/config/builder.rs`.
  Case: implementation incomplete relative to the stated future design (LISTEN/NOTIFY or unified process model).
