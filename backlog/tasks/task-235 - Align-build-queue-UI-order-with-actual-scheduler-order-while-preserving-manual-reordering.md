---
id: TASK-235
title: >-
  Align build queue UI order with actual scheduler order while preserving manual
  reordering
status: Backlog
assignee: []
created_date: '2026-04-01 02:30'
labels:
  - build-queue
  - scheduler
  - ui
  - backend
  - sprint-ready
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem Statement
The build queue shown in UI does not consistently match the order jobs are actually claimed by builders. This makes operators distrust queue position and can lead to incorrect intervention decisions.

## Goal
Make build queue visuals reflect the true effective execution order used by the scheduler, while preserving operator ability to manually reorder queue items and have that manual intent applied deterministically.

## Non-Goals
- No redesign of build worker execution engine beyond order semantics needed for correctness.
- No removal of priority weighting unless explicitly required by agreed ordering model.
- No changes to eval queue behavior (this task is build queue only).
- No UI aesthetic overhaul unrelated to queue order clarity.

## Scope
- Define and implement a single source-of-truth ordering model for build queue display and builder claim path.
- Ensure API returns stable queue position/order keys matching scheduler behavior.
- Add/maintain manual reorder controls that update persisted order and are honored by claim logic.
- Resolve tie-break behavior explicitly (e.g., priority, manual rank, created_at) and document it.
- Update UI labels/tooltips so operators understand when order is manual vs automatic.

## Architectural Constraints
- Queue ordering business logic must live in backend query/scheduler layers, not UI-only sorting.
- Manual reorder state must be persisted and transactionally safe.
- Avoid hidden global mutable state; maintain deterministic ordering across concurrent builders.

## Verification Plan
- Backend test: claim order matches returned queue order for representative mixed-priority jobs.
- Backend test: manual reorder changes persisted rank and subsequent claims follow new order.
- Concurrency test: multiple builder claims do not violate ordering invariants.
- UI test: displayed queue order and position values reflect backend order keys after reorder action.
- Targeted checks:
  - `nix develop -c env SQLX_OFFLINE=true cargo test --package crystal-forge <build-queue-order-tests>`
  - `nix develop -c env SQLX_OFFLINE=true cargo check --package crystal-forge`
  - `nix build .#checks.x86_64-linux.server --no-link`
  - `nix build .#checks.x86_64-linux.web-ui --no-link`

## Impact Areas
- `packages/default/src/queries/build_jobs.rs`
- `packages/default/src/queries/dashboard.rs`
- `packages/default/src/handlers/api/*` build queue endpoints
- `packages/web-ui/src/views/builds.rs` and related build queue components
- Associated migrations if persisted queue-rank fields are introduced/changed

## Risk Level
High (ordering bugs can starve jobs or mislead operators).

## Dependencies
- Coordinate with existing build reservation/claim locking semantics to preserve correctness under parallel workers.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 For queued jobs, UI order matches the actual next-claim order used by builders under the same state.
- [ ] #2 Manual reorder action updates persisted order and subsequent builder claims honor that order unless preempted by documented higher-priority rules.
- [ ] #3 Queue positions shown in UI are stable, deterministic, and derived from backend source-of-truth ordering keys.
- [ ] #4 Automated tests cover default ordering, manual reorder, and concurrent claim scenarios.
- [ ] #5 Task notes document final ordering precedence (e.g., manual rank > priority > created_at) and operator-visible behavior.
<!-- AC:END -->
