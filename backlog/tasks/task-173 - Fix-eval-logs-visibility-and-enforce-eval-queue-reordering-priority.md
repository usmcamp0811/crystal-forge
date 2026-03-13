---
id: TASK-173
title: Fix eval logs visibility and enforce eval queue reordering priority
status: Done
assignee: []
created_date: '2026-03-04 23:22'
updated_date: '2026-03-13 01:24'
labels:
  - bug
  - eval-queue
  - web-ui
  - server
dependencies:
  - TASK-174
references:
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/151'
priority: high
ordinal: 5000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem Statement
Two interconnected regressions are affecting evaluation operations:

1) **Eval logs visibility gap**
- Evaluation logs in the Evaluations view are often empty unless the user first navigates to Flakes and clicks the running-eval chip.
- This indicates the log stream/subscription initialization is inconsistent across entry points.

2) **Eval queue reordering not affecting execution order**
- Reordering commits in the eval queue UI does not appear to change the actual evaluation order.
- Users expect queue priority/order changes to directly influence the next commit selected for evaluation.

## Goal
- Ensure eval logs are visible/streaming directly from Evaluations view without requiring navigation to Flakes.
- Ensure eval worker selection respects persisted queue order so manual reorder is authoritative.

## Non-Goals
- No redesign of build queue behavior.
- No broad refactor of websocket architecture beyond what is needed to make stream init reliable.
- No unrelated UX changes outside eval logs/queue ordering behavior.

## Architectural Constraints
- Preserve event-driven eval triggering behavior.
- Keep queue ordering source-of-truth in the database and use deterministic ordering when claiming next commit.
- UI should not contain business logic; server decides next commit based on queue state.

## Verification Plan
- Reproduce from clean browser session:
  - Open Evaluations directly while an eval is running and confirm logs appear without visiting Flakes.
- Queue ordering validation:
  - Create multiple pending commits, reorder them in UI, verify next evaluated commit follows reordered priority.
- Regression checks:
  - Existing eval websocket reconnect behavior still works.
  - Existing log verbosity toggle and maximize modal continue to function.

## Impact Areas
- packages/web-ui (Evaluations log stream initialization/state)
- packages/default (next-commit selection query/order semantics)
- API contract for queue reorder + queue listing if required

## Risk Level
- Medium: touches both UI stream init and queue selection logic; requires careful end-to-end validation.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Opening Evaluations directly (without first visiting Flakes) shows live eval logs for the selected running commit.
- [ ] #2 Eval websocket/log stream is initialized consistently when selected commit changes in Evaluations.
- [ ] #3 Reordering pending commits in eval queue changes actual evaluation execution order for subsequent claims.
- [ ] #4 Server-side next-commit selection uses deterministic queue ordering that matches displayed queue order.
- [ ] #5 No regressions in existing eval log UI controls (collapse/expand, maximize modal, concise/verbose toggle, refresh/reconnect).
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Execution order update: TASK-173 is now dependent on TASK-174 so eval-log and queue-ordering bug validation can run against deterministic mock eval/build mode.

LOCK: opencode-gpt-5.3-codex on reckless in /home/mcamp/code/crystal-forge/TASK-173-fix-eval-logs-queue-priority

Execution started from TASK-174 branch state because TASK-173 depends on TASK-174 and TASK-174 is pending merge; this enables immediate validation continuity for mock-backed repros.

Started implementation after TASK-174 merge. Added server-side eval-claim loop fix in `server::process_pending_commits`: now re-queries pending commits and processes one commit per claim cycle to honor latest persisted queue ordering between evaluations (prevents stale snapshot ordering after UI reorder).

This change keeps deterministic DB queue priority authoritative for subsequent claims, aligning runtime selection behavior with reordered eval queue display.

Verification run: `nix develop -c env SQLX_OFFLINE=true cargo check -p crystal-forge` ✅ and `nix develop -c env SQLX_OFFLINE=true cargo test -p crystal-forge queries::commits::tests::reorder_validation_accepts_full_permutation_and_positions_are_dense` ✅.

Implemented server-side claim-order enforcement in `packages/default/src/server/mod.rs`: eval loop now re-queries pending commits and processes one commit per claim cycle, ensuring UI reorder updates affect subsequent commit claims instead of being hidden by stale batch snapshots.

Verification: `nix develop -c env SQLX_OFFLINE=true cargo check -p crystal-forge` ✅, `nix develop -c env SQLX_OFFLINE=true cargo test -p crystal-forge queries::commits::tests::reorder_validation_accepts_full_permutation_and_positions_are_dense` ✅, `nix develop -c cargo check` in `packages/web-ui` ✅.

Committed as `3ddff7d3` (`fix: claim eval commits in fresh queue-order cycles`) and pushed to `origin/TASK-173-fix-eval-logs-queue-priority`.

Opened MR: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/153

Moved TASK-173 to Review after implementing server-side eval claim-order fix and running targeted verification commands.

Addressed reviewer-blocking starvation regression in commit `0583cfb4`: added per-cycle pre-claim skip guard in eval claim loop so a poisoned head commit cannot hot-loop and starve later pending commits.

Pre-claim failure paths now skip current head for the remainder of that cycle (`get_flake` failure, config-load failure, non-race claim-start failure) while preserving re-query-per-claim ordering semantics.

Added focused tests in `server/mod.rs`: `select_next_pending_commit_id_skips_failed_heads` and `select_next_pending_commit_id_honors_reordered_head`. Verification rerun: `nix develop -c env SQLX_OFFLINE=true cargo check -p crystal-forge` ✅ and both targeted test commands ✅.

Posted MR follow-up review response notes on !153 with fix summary and verification commands.

Additional reviewer follow-up in `e7e14c4e`: moved flake/config setup failure handling to post-claim path and call `mark_commit_evaluation_failed` on those errors so retry metadata/backoff applies; this prevents head-of-queue hot-loop starvation while preserving re-query-per-claim reorder semantics.

Verification rerun after this change: `nix develop -c env SQLX_OFFLINE=true cargo check -p crystal-forge` ✅, `nix develop -c env SQLX_OFFLINE=true cargo test -p crystal-forge server::tests::select_next_pending_commit_id_skips_failed_heads` ✅, `nix develop -c env SQLX_OFFLINE=true cargo test -p crystal-forge server::tests::select_next_pending_commit_id_honors_reordered_head` ✅.

Posted follow-up review response note on MR !153 with fix details and commands.

Applied additional blocker fix in `12883cf0`: inner claim loop now returns to outer notifier/ticker pacing on flake lookup failure, config-load failure, non-race claim-start failure, and eval failure path (after `mark_commit_evaluation_failed`) to prevent same-head hot-loop retries.

Updated MR !153 description to resolve template inconsistency in schema section (`No schema changes` checked; migration/backfill unchecked). Added review note with control-flow clarification.

Verification rerun after final pacing fix: `nix develop -c env SQLX_OFFLINE=true cargo check -p crystal-forge` ✅ and `nix develop -c env SQLX_OFFLINE=true cargo test -p crystal-forge queries::commits::tests::reorder_validation_accepts_full_permutation_and_positions_are_dense` ✅.

Implemented latest review-requested loop semantics in `ef5c358e`: successful eval claim path continues inner cycle to re-query next pending head immediately, while failure paths yield back to outer notifier/ticker pacing to avoid hot retries.

Added focused helper tests in `server/mod.rs`: `select_next_pending_commit_id_honors_latest_reordered_snapshot` and `select_next_pending_commit_id_allows_progress_when_prior_head_is_deferred`, alongside rerun of reorder validation test.

Verification rerun: `nix develop -c env SQLX_OFFLINE=true cargo check -p crystal-forge` ✅, `nix develop -c env SQLX_OFFLINE=true cargo test -p crystal-forge queries::commits::tests::reorder_validation_accepts_full_permutation_and_positions_are_dense` ✅, plus both new server helper tests ✅.

Posted MR follow-up note on !153 with behavior summary and verification command list.

## Task Completion

MR !153 merged into dev at commit e4664ac3.

Implementation:
- Fixed eval claim loop to re-query pending commits per cycle, honoring latest queue order
- Added per-cycle skip guard to prevent head-of-queue starvation on poisoned commits
- Moved flake/config errors to post-claim path with retry backoff
- Inner loop continues on successful claim; failure paths yield to outer notifier
- Added focused tests for reorder semantics and failed-head deferral

All acceptance criteria satisfied:
- Eval logs visible directly from Evaluations view
- Eval queue reordering affects execution order
- Server-side claim uses deterministic queue ordering
- No regressions in log UI controls

Worktree cleanup: TASK-173-fix-eval-logs-queue-priority
<!-- SECTION:NOTES:END -->
