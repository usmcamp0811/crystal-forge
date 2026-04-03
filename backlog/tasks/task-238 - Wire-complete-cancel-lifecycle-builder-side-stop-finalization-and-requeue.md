---
id: TASK-238
title: 'Wire complete cancel lifecycle: builder-side stop, finalization, and requeue'
status: To Do
assignee: []
created_date: '2026-04-02 12:46'
updated_date: '2026-04-03 00:56'
labels:
  - builds
  - backend
  - builder
  - cancel-lifecycle
dependencies:
  - TASK-237
  - TASK-239
  - TASK-240
references:
  - packages/default/src/bin/builder.rs
  - packages/default/src/handlers/api/builders.rs
  - packages/default/src/queries/builders.rs
  - packages/default/src/queries/build_jobs.rs
  - packages/default/src/derivations/build.rs
  - packages/default/src/bin/server.rs
  - packages/web-ui/src/api/client.rs
  - packages/web-ui/src/views/builds.rs
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

TASK-237 added the `Cancelling` DB state and the admin cancel endpoint, but two things are missing that make the cancel feature incomplete from an operator perspective:

**1. The builder runtime never observes `cancelling`.** When an admin cancels a building job:
- The server sets `status = 'cancelling'` in the DB.
- The running nix build process continues unaffected — the builder has no mechanism to detect the state change.
- The builder eventually finishes and calls `complete_job` or `fail_job`, which **silently overwrites `cancelling` with `success` or re-queues it as failed**. The cancel had no operational effect.
- `finalize_cancelled_job()` exists in `queries/builders.rs` but is called from nowhere — no route, no builder-side caller.

**2. A cancelled job cannot be put back in the queue.** Once a job reaches `cancelled` there is no supported path to re-queue it. The "Restart" button currently calls `request_system_sync` (a full flake re-eval), which will not create a new job anyway because of the `NOT EXISTS (SELECT 1 FROM build_jobs WHERE derivation_id = d.id)` guard — the existing cancelled row blocks it. Operators who cancel a build by accident or want to retry it have no recovery path.

## Goal

Deliver a complete, end-to-end cancel lifecycle:
1. Cancel actually stops the running nix process within a predictable time window.
2. The job reaches `cancelled` with a `completed_at` timestamp, not `success` or re-queued `failed`.
3. A cancelled job can be explicitly re-queued by an operator without triggering a full flake re-eval.

## Non-Goals

- No changes to the frontend status display (already handled in TASK-237).
- No changes to the legacy direct-DB builder path (`builder/worker.rs`) — only the API-mode builder in `bin/builder.rs` is in scope.
- No changes to the normal failure retry path (`mark_job_failed_with_retry`).
- No flake re-evaluation or new derivation creation — requeue reuses the existing `build_jobs` row.

## Scope

### 1. Builder cancel-check polling

In `execute_build_job()` (`packages/default/src/bin/builder.rs`), add a cancel-check that runs on the existing heartbeat interval (every 30 s) using the same server roundtrip. On each heartbeat, also fetch the current job status from the server. If the returned status is `cancelling`, break the build loop and proceed to graceful termination.

Concrete approach: extend the existing `send_heartbeat()` call or add a lightweight `GET /api/v1/builders/{builder_id}/jobs/{job_id}` status fetch alongside it. No separate timer needed — piggyback on the 30 s heartbeat tick.

### 2. Graceful process termination

When the builder detects `cancelling`, the kill sequence:
1. Append a log line: `"[crystal-forge] Build cancelled by operator — stopping nix process"`.
2. Send `SIGTERM` to the child process.
3. Wait up to 30 s for the process to exit cleanly.
4. If still running, send `SIGKILL`.
5. Call the `finalize-cancelled` endpoint (see §3).

Implement as `graceful_kill(child: Child, timeout: Duration) -> std::process::ExitStatus` in a small helper, so it can be unit-tested independently of the build loop.

### 3. Finalize-cancelled endpoint + builder caller

Add a builder-authenticated endpoint:

```
POST /api/v1/builders/{builder_id}/jobs/{job_id}/finalize-cancelled
```

This calls the existing `finalize_cancelled_job()` query (`queries/builders.rs` line 951), which sets `status = 'cancelled'` and `completed_at = now()` where `status = 'cancelling'`.

Register the route in `bin/server.rs`. The builder calls this after the nix process has been killed and any log flushing is complete.

### 4. Job status check endpoint

Add (or reuse) a builder-authenticated:

```
GET /api/v1/builders/{builder_id}/jobs/{job_id}
```

Returns the `BuildJob` row. Used by the builder during the heartbeat loop to detect `cancelling`. If a suitable endpoint already exists that returns job status, reuse it.

### 5. Requeue cancelled job

Add a query `requeue_cancelled_job(pool, job_id)` that resets a `cancelled` job back to the queue **in-place** (updating the existing `build_jobs` row, not inserting a new one):

```sql
UPDATE build_jobs
SET status = 'queued',
    builder_id = NULL,
    started_at = NULL,
    completed_at = NULL,
    retry_count = 0,
    priority_weight = <original or recalculated>,
    updated_at = now()
WHERE id = $1
  AND status = 'cancelled'
RETURNING *
```

Updating the existing row avoids the `NOT EXISTS` duplicate guard that blocks new job creation for derivations that already have a `build_jobs` row.

Add a corresponding admin/operator endpoint:

```
POST /api/v1/build-jobs/{id}/requeue
```

Register the route in `bin/server.rs`.

### 6. Wire "Restart" button to requeue endpoint

In `packages/web-ui/src/views/builds.rs`, the `BuildAction::Restart` handler currently calls `request_system_sync`. For jobs that have a known `job_id` and a terminal status (`cancelled`, `failed`), it should instead call the new `POST /api/v1/build-jobs/{id}/requeue` endpoint. This is faster (no flake re-eval roundtrip), correct (reuses the known derivation), and avoids the duplicate-guard problem.

Update `packages/web-ui/src/api/client.rs` to add a `requeue_build_job(job_id)` function.

## Architectural Constraints

- Cancel-check polling must not introduce excessive server load — use the existing 30 s heartbeat interval, do not add a new faster timer.
- The builder must not overwrite `cancelling` with `success` — after detecting cancellation, the normal complete/fail paths must be skipped.
- Requeue must update the existing row, not insert a new one — the `NOT EXISTS` guard in `create_build_jobs_for_commit` and `enqueue_build_job_for_derivation` must remain untouched.
- The `finalize-cancelled` and job-status endpoints must be builder-authenticated, not admin-authenticated.
- The `requeue` endpoint should require operator or admin (consistent with TASK-240 resolution).
- `finalize_cancelled_job` and `requeue_cancelled_job` queries must be idempotent — safe to call multiple times.

## Verification Plan

### Tier 0
- `nix develop -c env SQLX_OFFLINE=true cargo check --package crystal-forge`
- `nix develop -c cargo check --package crystal-forge-ui`
- Unit test: `graceful_kill` sends SIGTERM, waits, then SIGKILL on timeout.
- Unit test: `cancel_build_job` transitions `building → cancelling`, `finalize_cancelled_job` transitions `cancelling → cancelled`.
- Unit test: `requeue_cancelled_job` transitions `cancelled → queued`, clears `builder_id`/`started_at`/`completed_at`.
- Unit test: `requeue_cancelled_job` rejects non-cancelled statuses.

### Tier 1
- Start a real build via `server-stack-mock up`.
- Cancel it via `POST /api/v1/build-jobs/{id}/cancel`.
- Verify: job transitions to `cancelling` in DB.
- Verify: within ~30 s, the builder kills the nix process, appends a cancellation log line, and calls `finalize-cancelled`.
- Verify: job reaches `cancelled` in DB with a `completed_at` timestamp and the cancellation log visible in the build log view.
- Verify: clicking Restart on the cancelled job calls `requeue` and the job appears back in the queue as `queued` without triggering a flake sync.
- Verify: clicking cancel on a **queued** job (not building) still works as before — immediate `cancelled`, no builder interaction needed.

## Impact Areas

- `packages/default/src/bin/builder.rs` — cancel-check in job execution loop, graceful_kill helper
- `packages/default/src/handlers/api/builders.rs` — finalize-cancelled handler, job status handler, requeue handler
- `packages/default/src/queries/builders.rs` — requeue_cancelled_job query, finalize_cancelled_job already exists
- `packages/default/src/bin/server.rs` — new route registrations
- `packages/web-ui/src/api/client.rs` — requeue_build_job client function
- `packages/web-ui/src/views/builds.rs` — Restart action handler

## Risk Level

High — touches the running build process termination path and job state machine. Must not allow a cancelled job to be silently completed or permanently stuck.

## Dependencies

- TASK-237 MR !205 (introduces `cancelling`/`cancelled` status and the cancel endpoint — must be merged first, or changes layered on the same branch)
- TASK-239 (log append gate for `cancelling` — should land in same sprint so shutdown logs are visible)
- TASK-240 (role contract decision — determines whether requeue endpoint is admin-only or operator+)
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Cancelling a building job stops the nix process within one heartbeat interval (~30 s); the job reaches `cancelled` with a `completed_at` timestamp and does NOT silently complete as `success`.
- [ ] #2 The builder appends a human-readable cancellation log line before killing the process, visible in the build log view.
- [ ] #3 Cancelling a queued job still works immediately (existing behaviour preserved).
- [ ] #4 A `cancelled` job can be requeued via `POST /api/v1/build-jobs/{id}/requeue`; the existing `build_jobs` row is updated in-place (no new row inserted), status returns to `queued`, and `builder_id`/`started_at`/`completed_at` are cleared.
- [ ] #5 The Restart button for cancelled (and failed) jobs calls the requeue endpoint instead of triggering a flake sync.
- [ ] #6 The `finalize-cancelled` and job-status endpoints are builder-authenticated; the `requeue` endpoint enforces operator-or-admin (consistent with TASK-240).
- [ ] #7 Unit tests cover: graceful_kill SIGTERM→SIGKILL sequence, cancel/finalize state transitions, requeue state transition, requeue rejection for non-cancelled statuses.
- [ ] #8 Tier 1 end-to-end test passes: cancel a live building job, verify cancellation log, verify `cancelled` state, verify requeue puts it back without flake re-eval.
<!-- AC:END -->
