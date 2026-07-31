---
id: TASK-239
title: Allow log appends while build job status is cancelling
status: Backlog
assignee: []
created_date: '2026-04-02 12:47'
labels:
  - builds
  - backend
  - builder
  - cancel-lifecycle
dependencies:
  - TASK-237
  - TASK-238
references:
  - packages/default/src/handlers/api/builders.rs
  - packages/default/src/queries/builders.rs
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

The builder log-append path (`POST /api/v1/builders/{builder_id}/jobs/{job_id}/logs` and the corresponding DB function `append_job_logs_with_limits`) rejects any log append when the job status is not `queued` or `building`:

```rust
// packages/default/src/handlers/api/builders.rs ~line 752
if job.status != "queued" && job.status != "building" {
    return Err((StatusCode::CONFLICT, ...));
}
```

The DB-level function enforces the same gate in SQL.

When a job transitions to `cancelling`, the builder has shutdown/cleanup logs that are the most operationally valuable — they describe why the stop was requested, what the nix process reported on exit, and whether cleanup succeeded. Blocking these logs makes the cancellation lifecycle invisible in the build log view.

## Goal

Allow log appends while a job's status is `cancelling`. Maintain the existing rejection for `cancelled`, `success`, and `failed` (terminal states where logs are final).

## Non-Goals

- No changes to log retention or size limits.
- No changes to WebSocket streaming behavior.
- No UI changes.

## Scope

### 1. Handler status gate

In `packages/default/src/handlers/api/builders.rs`, function `append_job_logs()` (~line 752), extend the allowed statuses:

```rust
// Before
if job.status != "queued" && job.status != "building" {

// After
let active_statuses = ["queued", "building", "cancelling"];
if !active_statuses.contains(&job.status.as_str()) {
```

### 2. DB-level SQL gate

In `packages/default/src/queries/builders.rs`, function `append_job_logs_with_limits()` (~line 776), update the `WHERE status IN (...)` clause:

```sql
-- Before
AND status IN ('queued', 'building')

-- After
AND status IN ('queued', 'building', 'cancelling')
```

### 3. WebSocket streaming handler

Review the WebSocket log streaming handler (`stream_build_logs()` in `handlers/api/builders.rs` ~line 849). It calls the same underlying append function — confirm it picks up the gate fix automatically. If it has its own status check, update it to match.

## Architectural Constraints

- `cancelling` is explicitly an active/in-flight state — logs during it are part of the job's observable history.
- Terminal states (`cancelled`, `success`, `failed`) must still reject appends.
- The fix must not allow the builder to resurrect a `cancelled` or `failed` job by appending logs.

## Verification Plan

### Tier 0
- `nix develop -c env SQLX_OFFLINE=true cargo check --package crystal-forge`
- Unit/integration test: attempt to append a log chunk to a job in each status; verify `cancelling` succeeds and terminal states still reject.

### Tier 1 (integration with TASK-238)
- Cancel a building job; confirm shutdown logs from the builder appear in the build log view.

## Impact Areas

- `packages/default/src/handlers/api/builders.rs` — log append status gate
- `packages/default/src/queries/builders.rs` — SQL status gate in `append_job_logs_with_limits`

## Risk Level

Low — a targeted gate expansion with no schema changes.

## Dependencies

- TASK-237 MR !205 (introduces `cancelling` status)
- TASK-238 (builder-side cancel detection — together these form a complete cancel flow)
<!-- SECTION:DESCRIPTION:END -->
