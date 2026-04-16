---
id: TASK-273
title: Add evaluation cancellation and history view mirroring builds UI
status: Backlog
assignee: []
created_date: '2026-04-16 01:24'
labels:
  - evaluation
  - ui
  - ux
  - admin
milestone: CVE Workflow Improvements
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

Three related gaps in the evaluation workflow:

1. **Stuck evaluations cannot be cancelled.** Once a commit enters `in_progress` evaluation, there is no way to cancel it short of restarting the server. Users are seeing evaluations hang indefinitely.

2. **No cancel action for pending evaluations.** A commit sitting in the eval queue with status `pending` also cannot be removed without a server restart.

3. **No eval history view.** The builds page has a tabbed view showing active queue + full history with pagination and filtering. The evaluations page only shows the active queue — there is no way to review past evaluations, their durations, error messages, or outcomes.

## Goal

- Add cancel support for evaluations (both `pending` and `in_progress`), mirroring the two-level cancel pattern used for builds (`cancel` for graceful, `force-cancel` for stuck).
- Add an eval history tab/view that mirrors the builds page appearance: same tab structure, same status chips, same pagination and filtering style, showing completed/failed/cancelled evaluations with their commit hash, flake, duration, and outcome.

## Non-Goals

- Do not change how `nix-eval-jobs` is invoked or its subprocess management beyond adding a cooperative cancellation signal.
- Do not change the eval retry/backoff logic for failed evals.
- Do not add eval history to the dashboard summary widget (separate task if needed).
- Do not paginate the active queue (already small).

## Acceptance Criteria
- [ ] #1 `pending` eval can be cancelled immediately via `POST /api/v1/commits/:id/cancel-evaluation` → status transitions to `cancelled`
- [ ] #2 `in_progress` eval can be requested for cancellation → transitions to `cancelling`, eval loop detects the flag and kills the nix-eval-jobs subprocess, then transitions to `cancelled`
- [ ] #3 A `force-cancel` endpoint exists for evals stuck in `cancelling` — mirrors `POST /api/v1/build-jobs/:id/force-cancel`
- [ ] #4 `cancelled` is added as a valid value for `commits.evaluation_status` via a new migration
- [ ] #5 A `cancellation_requested` flag (or equivalent) is added to `commits` table so the running eval loop can cooperatively detect and honour cancel requests
- [ ] #6 The eval queue UI shows a Cancel button on each pending and in-progress row in the Active section
- [ ] #7 The Cancel button is disabled / shows a spinner while the request is in-flight
- [ ] #8 Eval history tab added to the evaluations page, mirroring the builds page tab structure (Active / History tabs)
- [ ] #9 History tab shows: commit hash (truncated, linked), flake name, started_at, duration, status chip, error message (collapsed) — matching the visual style of the builds history tab
- [ ] #10 History tab supports filtering by status (complete / failed / cancelled) and flake, matching the builds filter bar pattern
- [ ] #11 History tab is paginated (server-side), matching the builds page pagination pattern
- [ ] #12 All new API endpoints are admin/operator-only, matching the build cancel auth pattern
- [ ] #13 New migration does not modify any existing migration file

## Architectural Constraints

### Backend

**New migration** (e.g. `0113_add_eval_cancellation_support.sql`):
- Add `'cancelled'` and `'cancelling'` to the CHECK constraint on `commits.evaluation_status`
- Add `cancellation_requested BOOLEAN NOT NULL DEFAULT FALSE` column to `commits`

**New query functions** in `packages/default/src/queries/commits.rs`:
- `cancel_commit_evaluation(pool, commit_id)` — `pending → cancelled` (immediate), `in_progress → cancelling` (sets flag)
- `force_cancel_commit_evaluation(pool, commit_id)` — `cancelling → cancelled` (no subprocess check)
- `list_eval_history(pool, params)` — paginated query for `complete | failed | cancelled` commits with duration, flake, commit hash

**New HTTP handlers** in `packages/default/src/handlers/api/commits.rs`:
- `POST /api/v1/commits/:id/cancel-evaluation`
- `POST /api/v1/commits/:id/force-cancel-evaluation`
- `GET /api/v1/commits/eval-history` (paginated, filterable)

**Eval loop change** in `packages/default/src/server/mod.rs` (or wherever `run_commit_evaluation_loop` / `evaluate_with_nix_eval_jobs` lives):
- Periodically poll `cancellation_requested` on the active commit during eval
- On detection: kill subprocess, transition `cancelling → cancelled`

### Frontend

**New API client functions** in `packages/web-ui/src/api/client.rs`:
- `cancel_commit_evaluation(commit_id)`
- `force_cancel_commit_evaluation(commit_id)`
- `fetch_eval_history(params)` — paginated

**UI changes** in `packages/web-ui/src/views/evaluations.rs`:
- Add Cancel / Force-Cancel buttons on active queue rows
- Add History tab mirroring the builds page tab structure (`packages/web-ui/src/views/builds.rs`)
- History list component matching the builds history card visual style
- Filter bar matching builds filter bar (status chip filter + flake name input)
- Pagination controls matching builds pagination

## Impact Areas

- `packages/default/migrations/` — new `0113_...`
- `packages/default/src/queries/commits.rs` — new query functions + history query
- `packages/default/src/handlers/api/commits.rs` — new cancel + history handlers
- `packages/default/src/bin/server.rs` — new routes
- `packages/default/src/server/mod.rs` — cooperative cancellation in eval loop
- `packages/default/src/api/models.rs` — extend `EvaluationStatus` enum if present
- `packages/web-ui/src/api/client.rs` — new client functions
- `packages/web-ui/src/views/evaluations.rs` — cancel buttons + history tab

## Verification Plan

**Tier 0:**
```
SQLX_OFFLINE=true cargo check
SQLX_OFFLINE=true cargo test --lib
```

**Tier 1:**
```
db-only up
cargo sqlx prepare
server-stack-mock up
```
- Manually cancel a pending eval → status chip shows `cancelled`
- Restart server with a stuck eval → `reset_stuck_commit_evaluations` leaves `cancelled` rows alone (must not reset them to `pending`)
- History tab shows completed and failed evals with correct duration and status chip
- Pagination works; filter by `failed` shows only failed rows

## Risk Level

**Medium-High.**
- Killing a subprocess mid-eval requires care to avoid orphaned nix store paths or partial derivation rows — `cleanup_partial_derivations` already handles this at startup but the cooperative path needs the same cleanup.
- The unique partial index on `in_progress` (`idx_commits_single_in_progress`) must be extended or adjusted to allow `cancelling` as a transitional exclusive state.
- `reset_stuck_commit_evaluations` on server startup must NOT reset `cancelled` or `cancelling` rows back to `pending`.
<!-- SECTION:DESCRIPTION:END -->
