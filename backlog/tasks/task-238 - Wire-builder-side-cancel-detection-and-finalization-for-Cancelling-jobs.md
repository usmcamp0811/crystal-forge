---
id: TASK-238
title: Wire builder-side cancel detection and finalization for Cancelling jobs
status: Backlog
assignee: []
created_date: '2026-04-02 12:46'
labels:
  - builds
  - backend
  - builder
  - cancel-lifecycle
dependencies: []
references:
  - packages/default/src/bin/builder.rs
  - packages/default/src/handlers/api/builders.rs
  - packages/default/src/queries/builders.rs
  - packages/default/src/derivations/build.rs
  - packages/default/src/bin/server.rs
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

TASK-237 added the `Cancelling` DB state and the admin cancel endpoint, but the builder runtime never observes it. When an admin cancels a building job:

1. The server sets `status = 'cancelling'` in the DB.
2. The running nix build process continues unaffected — the builder has no mechanism to detect the state change.
3. `finalize_cancelled_job()` exists in `queries/builders.rs` but is called from **nowhere** — there is no route, no builder-side caller, no handler registered for it.
4. The only way the job eventually stops is the 2-hour timeout, at which point it is killed via tokio `Child` drop (SIGKILL), not a graceful stop.

The `building → cancelling → cancelled` flow described in the task notes and MR is a promise that is currently undelivered at the builder runtime level.

## Goal

Make the builder runtime actually detect a `cancelling` state transition, stop the running nix build process, and call `finalize_cancelled_job` to complete the lifecycle.

## Non-Goals

- No changes to the frontend or UI state display (already handled in TASK-237).
- No redesign of the reservation/claim system.
- No changes to the legacy direct-DB builder path (only the API-mode builder in `bin/builder.rs` is in scope, as it is the forward path).

## Scope

### 1. Builder cancel-check polling

The API-mode builder (`packages/default/src/bin/builder.rs`) needs a cancel-check mechanism during `execute_build_job()`. Options:

**Option A (polling):** During the build heartbeat loop, periodically query the server for the current job status. If `cancelling` is detected, kill the child process and break.

**Option B (server-push):** Add a server-to-builder cancel signal over the existing WebSocket log stream or a new channel.

**Recommended: Option A** — polling is simpler, fits the existing heartbeat pattern, and avoids a new WebSocket direction. The builder already sends a heartbeat to the server every 30 s; we can add a status check on the same interval.

Concrete change: In `execute_build_job()` (`bin/builder.rs`), add a cancel-check inside the build execution loop that calls a new `GET /api/v1/builders/{builder_id}/jobs/{job_id}/status` endpoint (or reuses an existing job fetch) every 15–30 s. If the returned status is `cancelling`, kill the child process gracefully (SIGTERM then SIGKILL after timeout) and break the loop.

### 2. Builder finalize-cancelled endpoint + caller

Add a builder-authenticated `POST /api/v1/builders/{builder_id}/jobs/{job_id}/finalize-cancelled` endpoint that calls `finalize_cancelled_job()`.

Register the route in `bin/server.rs`.

The builder calls this endpoint after the build process has been killed and any cleanup (log flush, temp file removal) is complete.

### 3. Job status check endpoint

Add `GET /api/v1/builders/{builder_id}/jobs/{job_id}` (or reuse an existing endpoint) so the builder can poll whether its current job has been externally cancelled. The response should include at minimum the job `status` field.

### 4. Graceful process termination

When the builder detects `cancelling`, the termination sequence should be:
1. Log a message: "Build cancelled by operator, stopping nix process..."
2. Send `SIGTERM` to the child process and give it N seconds (e.g. 30 s) to exit.
3. If it hasn't exited, send `SIGKILL`.
4. Call `finalize-cancelled` endpoint.

Implement as a helper `graceful_kill(child, timeout)` that wraps tokio `Child`.

## Architectural Constraints

- Cancel detection must not introduce excessive server load — polling interval should be ≥ 15 s.
- The builder must not leave orphaned nix store partial outputs; existing cleanup logic (if any) should still run.
- The API endpoints must be builder-authenticated (not admin), since it is the builder that calls them.
- The `finalize_cancelled_job` query already exists in `queries/builders.rs` — do not duplicate it.

## Verification Plan

### Tier 0
- `nix develop -c env SQLX_OFFLINE=true cargo check --package crystal-forge`
- Unit test: `cancel_build_job` transitions `building → cancelling`, `finalize_cancelled_job` transitions `cancelling → cancelled`.
- Unit test: graceful_kill sends SIGTERM, then SIGKILL on timeout.

### Tier 1
- Start a real build (via `server-stack-mock up`).
- Hit `POST /api/v1/build-jobs/{id}/cancel` from admin.
- Verify: job transitions to `cancelling` in DB.
- Verify: within one polling interval, the builder kills the nix process and calls `finalize-cancelled`.
- Verify: job reaches `cancelled` in DB with a completion timestamp.

## Impact Areas

- `packages/default/src/bin/builder.rs` — job execution loop
- `packages/default/src/handlers/api/builders.rs` — new finalize-cancelled handler + job status endpoint
- `packages/default/src/queries/builders.rs` — finalize_cancelled_job already exists, new status fetch if needed
- `packages/default/src/bin/server.rs` — new route registrations

## Risk Level

High — touches the running build process termination path. Must not introduce data corruption or orphaned processes.

## Dependencies

- TASK-237 MR !205 (must be merged first, or changes can be layered on the same branch with reviewer approval)
<!-- SECTION:DESCRIPTION:END -->
