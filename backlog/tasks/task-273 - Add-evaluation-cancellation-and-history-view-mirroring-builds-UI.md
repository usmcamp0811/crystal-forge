---
id: TASK-273
title: Add evaluation cancellation and history view mirroring builds UI
status: In Progress
assignee: []
created_date: '2026-04-16 01:24'
updated_date: '2026-04-16 02:01'
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

1. **Stuck evaluations cannot be cancelled.** Once `in_progress`, no way to cancel short of server restart.
2. **No cancel for pending evals.** Queue cannot be cleared without restart.
3. **No eval history view.** Only the active queue is shown — no way to review past evals, durations, or errors.

## Goal

- Add cancel support for evaluations mirroring the two-level build cancel pattern (`cancel` / `force-cancel`).
- Add eval history tab mirroring the builds page: same tabs, status chips, pagination, filter bar.

## Non-Goals

- Do not change nix-eval-jobs invocation beyond adding a cooperative cancellation signal.
- Do not change eval retry/backoff logic for failed evals.
- Do not add eval history to dashboard summary widget.
- Do not paginate the active queue.

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 #1 `pending` eval can be cancelled immediately via `POST /api/v1/commits/:id/cancel-evaluation` → status transitions to `cancelled`
- [ ] #2 #2 `in_progress` eval can be requested for cancellation → transitions to `cancelling`, eval loop detects the flag and kills the nix-eval-jobs subprocess, then transitions to `cancelled`
- [ ] #3 #3 A `force-cancel` endpoint exists for evals stuck in `cancelling` — mirrors `POST /api/v1/build-jobs/:id/force-cancel`
- [ ] #4 #4 `cancelled` is added as a valid value for `commits.evaluation_status` via a new migration
- [ ] #5 #5 A `cancellation_requested` flag (or equivalent) is added to `commits` table so the running eval loop can cooperatively detect and honour cancel requests
- [ ] #6 #6 The eval queue UI shows a Cancel button on each pending and in-progress row in the Active section
- [ ] #7 #7 The Cancel button is disabled / shows a spinner while the request is in-flight
- [ ] #8 #8 Eval history tab added to the evaluations page, mirroring the builds page tab structure (Active / History tabs)
- [ ] #9 #9 History tab shows: commit hash (truncated, linked), flake name, started_at, duration, status chip, error message (collapsed) — matching the visual style of the builds history tab
- [ ] #10 #10 History tab supports filtering by status (complete / failed / cancelled) and flake, matching the builds filter bar pattern
- [ ] #11 #11 History tab is paginated (server-side), matching the builds page pagination pattern
- [ ] #12 #12 All new API endpoints are admin/operator-only, matching the build cancel auth pattern

## Key Implementation Notes

**Migration `0113`:**
- Extend CHECK constraint to include `cancelling` and `cancelled`
- Add `cancellation_requested BOOLEAN NOT NULL DEFAULT FALSE`
- Drop + recreate `idx_commits_single_in_progress` (currently `WHERE evaluation_status = 'in_progress'`) to `WHERE evaluation_status IN ('in_progress', 'cancelling')`

**Backend files:**
- `queries/commits.rs`: `cancel_commit_evaluation`, `force_cancel_commit_evaluation`, `list_eval_history`
- `api/models.rs`: `EvalHistoryItem`, `EvalHistoryPage`, `EvalHistoryParams`, `CancelEvalOutcome`
- `handlers/api/commits.rs`: 3 new handlers
- `bin/server.rs`: 3 new routes
- `server/mod.rs`: fix `reset_stuck_commit_evaluations` WHERE clause; wire cancellation poll
- `models/evaluate_with_policies.rs`: third `tokio::select!` arm polling `cancellation_requested` every ~2s; on detection `child.kill().await`, cleanup, return `Err`

**Frontend files:**
- `web-ui/src/api/client.rs`: 3 new client functions
- `web-ui/src/views/evaluations.rs`: `EvaluationsTab` enum, cancel buttons, history tab, filter bar, pagination — mirror `builds.rs` exactly

## Verification Plan

**Tier 0:** `SQLX_OFFLINE=true nix develop -c cargo check && cargo test --lib`

**Tier 1:** `db-only up && cargo sqlx prepare && server-stack-mock up`
- Cancel pending eval → `cancelled` chip
- Cancel in_progress eval → `cancelling` then `cancelled` within ~4s
- Force-cancel `cancelling` → immediate `cancelled`
- Server restart leaves `cancelled` rows untouched
- History tab loads; filters and pagination work

**Tier 2:** `nix flake check` before MR (migration + server startup changes).

## Risk

- `cleanup_partial_derivations` called at startup must also run inline on cooperative cancel — verify safe outside startup context.
- `re_evaluate_commit` has no auth guard (pre-existing, out of scope).

## Dependencies

Migration `0112` merged (TASK-272 ✅). Next number: `0113`.
<!-- SECTION:DESCRIPTION:END -->

- [ ] #13 #13 New migration does not modify any existing migration file
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
LOCK: claude on reckless in ~/code/crystal-forge/TASK-273-eval-cancel-history
<!-- SECTION:NOTES:END -->
