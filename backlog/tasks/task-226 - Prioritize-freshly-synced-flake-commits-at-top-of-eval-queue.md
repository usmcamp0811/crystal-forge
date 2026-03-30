---
id: TASK-226
title: Prioritize freshly synced flake commits at top of eval queue
status: To Do
assignee: []
created_date: '2026-03-30 01:56'
updated_date: '2026-03-30 01:57'
labels:
  - queueing
  - flakes
  - evaluation
  - scheduling
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem Statement

When a flake is manually synced/refreshed and new commits are discovered, those commits may wait behind older queued evaluations. This makes operator-triggered sync feel slow and delays feedback.

## Goal

Ensure newly discovered commits from explicit flake sync/refresh flows are placed at the front of the evaluation queue so evaluation starts immediately.

## Non-Goals

- No redesign of global scheduler policy.
- No changes to build queue prioritization.
- No unrelated refactors in commit ingestion paths.

## Architectural Constraints

- Keep queue-priority behavior localized to evaluation queue insertion/update logic.
- Preserve deterministic ordering among commits promoted by the same sync operation (newest first).
- Keep background/non-operator ingestion behavior unchanged unless explicitly required.

## Verification Plan

- Add/adjust unit/integration test(s) covering sync-created commits being prioritized ahead of existing queued commits.
- Verify queue positions/order from DB query path used by evaluator workers.
- Run targeted package tests and compile checks in Nix dev environment.

## Impact Areas

- `packages/default/src/handlers/api/flakes.rs`
- `packages/default/src/flake/commits.rs`
- `packages/default/src/queries/commits.rs` (or queue-related query module)
- Related tests in default package

## Risk Level

Medium (scheduler behavior change affecting evaluation ordering).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 After syncing a flake that yields new commits, those new commits are placed ahead of previously queued evaluations.
- [ ] #2 If multiple commits are added in one sync, their relative queue order is deterministic (newest-first or clearly documented intended order).
- [ ] #3 Evaluator worker selects the newly synced commit(s) before older queued items under equal priority conditions.
- [ ] #4 Existing non-sync ingestion paths remain unchanged unless required for correctness.
- [ ] #5 Targeted tests covering queue-priority behavior pass, and package checks succeed in nix dev environment.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Locate current evaluation queue ordering and where sync-inserted commits are enqueued.
2. Implement minimal queue-priority mechanism for commits inserted by Sync from Source / refresh-triggered sync.
3. Ensure deterministic ordering for multiple new commits (newest first at queue top).
4. Add regression test proving new sync commits preempt older queued commits.
5. Run targeted tests and checks; prepare MR notes with before/after queue behavior.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Sprint-ready notes:
- Prioritize explicit operator-triggered sync outcomes over passive queue order.
- Keep fairness implications scoped and documented.
- If broader queue policy issues are discovered, create follow-up Backlog tasks rather than expanding scope.
<!-- SECTION:NOTES:END -->

## Definition of Done
<!-- DOD:BEGIN -->
- [ ] #1 MR includes clear note of queue-order behavior change and rationale.
- [ ] #2 Any discovered broader scheduler/fairness follow-ups are tracked as separate Backlog tasks.
<!-- DOD:END -->
