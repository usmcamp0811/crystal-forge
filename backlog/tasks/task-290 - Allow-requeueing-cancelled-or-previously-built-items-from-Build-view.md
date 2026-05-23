---
id: TASK-290
title: 'Allow requeueing cancelled, failed, or successful builds from Build view'
status: Review
assignee: []
created_date: '2026-05-08 02:20'
updated_date: '2026-05-08 02:44'
labels:
  - feature
  - builds
  - ui
  - api
  - requeue
  - sprint-ready
milestone: Build Queue UX
dependencies: []
references:
  - /packages/web-ui/src/views/builds.rs
  - /packages/default/src/handlers/api
  - /packages/default/src/queries/build_jobs.rs
documentation:
  - /AGENTS.md
modified_files:
  - packages/web-ui/src/views/builds.rs
  - packages/default/src/handlers/api/*.rs
  - packages/default/src/queries/build_jobs.rs
  - packages/web-ui/src/components/builds/build_queue_pane.rs
  - packages/web-ui/src/api/client.rs
  - packages/default/src/queries/builders.rs
  - packages/default/src/handlers/api/builders.rs
priority: high
ordinal: 2900
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem Statement
Operators cannot currently trigger a rebuild from the Build view for terminal build outcomes (cancelled, failed, successful). This blocks recovery and re-validation workflows after transient infrastructure errors, cache issues, or post-change verification needs.

## Goal
Implement end-to-end requeue capability in Build view so eligible terminal builds can be requeued and executed again as a new build attempt.

## Decisions (Confirmed)
- Eligible statuses: cancelled, failed, success
- Attempt model: create a brand-new build attempt record (do not mutate prior attempt)
- UI placement: per-row action icon/menu in Build view
- Retry guardrail: no hard retry limit in this task
- Authorization: Operator or Admin
- Queue insertion: append new attempt to queue tail
- Scope depth: full-stack (UI + API)

## Non-Goals
- Bulk requeue UX
- Retry cooldowns or max-attempt throttling
- Priority override/reinsert-at-position controls
- Changes to queue reorder semantics beyond append-on-requeue
- Historical data backfill/migration for prior attempts

## Architectural Constraints
- Preserve immutable build attempt history (no state mutation on previous attempt records)
- New requeue attempts must be represented as new rows/attempts linked to original target context
- UI must remain presentation-focused; API/business logic stays server-side
- RBAC enforcement must occur server-side regardless of UI visibility

## Verification Plan
Tier 0 (targeted):
- API/handler tests for requeue endpoint behavior and RBAC
- Query-layer tests for attempt creation semantics
- UI tests/component checks for requeue action visibility + optimistic refresh behavior

Tier 1 (feature-level):
- Run server stack and manually validate:
  1) Requeue cancelled build => new queued attempt appears
  2) Requeue failed build => new queued attempt appears
  3) Requeue successful build => new queued attempt appears
  4) Viewer cannot requeue (403 / no action)
  5) Existing queue reorder/cancel still works

## Impact Areas
- Build view row actions and refresh logic
- Build-related API routes/handlers
- Build job persistence/query logic for new attempt creation
- Authorization checks for requeue action

## Risk Level
Medium:
- Queue integrity and ordering side effects if insertion semantics are wrong
- Potential duplicate rebuild pressure if users repeatedly requeue
- RBAC gaps could expose unintended queue control

## Dependencies
- No blocking upstream task dependency declared
- Must align with existing queue/cancel behavior contracts in current dev branch
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Build view displays a requeue row action for build entries with terminal statuses cancelled, failed, or success.
- [ ] #2 Requeue action is visible only to Operator/Admin-capable users in UI, and API rejects unauthorized callers with 403.
- [ ] #3 Invoking requeue creates a brand-new build attempt record (new id) without mutating existing attempt records.
- [ ] #4 New attempt is inserted at queue tail in queued state and is eligible for normal worker pickup.
- [ ] #5 Original build attempt remains intact in history with its original terminal status and timestamps.
- [ ] #6 Requeue works for each eligible terminal status: cancelled, failed, and success.
- [ ] #7 After requeue, UI refresh shows the new queued attempt and no corruption of queue ordering.
- [ ] #8 Existing cancel and queue reorder flows continue to work unchanged for other entries.
- [ ] #9 Failure paths (invalid status, missing target context, unauthorized caller) return explicit API errors and user-facing feedback.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Add server-side requeue API action guarded by Operator/Admin role.
2. Implement query/service logic to create a new queued build attempt from eligible prior attempt context.
3. Ensure queue insertion is tail-appended and previous attempts remain immutable.
4. Add Build view row action for eligible terminal statuses with success/error feedback.
5. Refresh queue/history state after requeue action.
6. Add targeted tests for eligibility, RBAC, and attempt creation semantics.
7. Run feature validation against local stack for cancelled/failed/success requeue flows.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Sprint-ready refinement completed with explicit goal, non-goals, constraints, verification plan, and risk profile.

Execution decisions captured: statuses=cancelled|failed|success; new attempt model; row action; no retry cap; Operator/Admin; append to tail; full-stack scope.

MR !252 created: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/252

Implemented backend requeue as new attempt row with immutable history and operator/admin RBAC

Implemented UI requeue visibility gating to operator-or-above users and relabeled action to Requeue

Verification: web-ui cargo check passes; default cargo check blocked by SQLx DB connectivity in current environment
<!-- SECTION:NOTES:END -->

## Definition of Done
<!-- DOD:BEGIN -->
- [ ] #1 Add/extend backend tests covering eligibility matrix and RBAC for requeue endpoint.
- [ ] #2 Add/extend query/service tests asserting new-attempt creation and immutable prior attempts.
- [ ] #3 Capture manual validation evidence for requeue from cancelled/failed/success in local stack run.
- [ ] #4 Add/extend backend tests covering eligibility matrix and RBAC for requeue endpoint.
- [ ] #5 Add/extend query/service tests asserting new-attempt creation and immutable prior attempts.
- [ ] #6 Capture manual validation evidence for requeue from cancelled/failed/success in local stack run.
- [ ] #7 Add/extend backend tests covering eligibility matrix and RBAC for requeue endpoint.
- [ ] #8 Add/extend query/service tests asserting new-attempt creation and immutable prior attempts.
- [ ] #9 Capture manual validation evidence for requeue from cancelled/failed/success in local stack run.
<!-- DOD:END -->
