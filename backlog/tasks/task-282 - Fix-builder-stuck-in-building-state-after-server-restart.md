---
id: TASK-282
title: Fix builder stuck in "building" state after server restart
status: In Progress
assignee:
  - '@openai-gpt-5.5'
created_date: '2026-04-20 19:22'
updated_date: '2026-06-26 01:21'
labels:
  - bug
  - builder
  - infrastructure
  - queue
  - recovery
milestone: Reliability
dependencies: []
references:
  - packages/default/src/
  - packages/default/src/services/
  - packages/default/src/handlers/
priority: high
ordinal: 11000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem
Builds can remain indefinitely in `building` after server restart or builder process loss, even when the builder is no longer running that job. This blocks queue progress and leaves stale in-flight state.

## Goal
Ensure orphaned in-progress builds are detected and recovered so the queue continues processing reliably.

## Non-Goals
- Re-architect the entire build scheduler
- Add new distributed coordination primitives
- Implement advanced alerting/metrics dashboards beyond minimal logs for this fix

## Chosen Scope
- Startup orphan recovery (required)
- Runtime crash/liveness recovery for builder disconnection/death (also included)

## Recovery Policy (chosen)
When orphaned `building` jobs are detected, transition them back to queued/pending for automatic retry.

## Architectural Constraints
- Keep build state transitions explicit and auditable
- Preserve existing queue ordering semantics
- Avoid introducing hidden global mutable state
- Keep changes scoped to queue/lifecycle management and related tests

## Impact Areas
- Server startup initialization
- Build queue lifecycle/state transitions
- Builder liveness handling during runtime
- Integration tests for restart/disconnect recovery

## Risk
High: incorrect state transition logic could duplicate work or regress queue behavior. Mitigate with targeted integration tests around restart + runtime disconnect flows.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 On server startup, any orphaned build rows in `building` with no active builder execution are transitioned to queued/pending.
- [ ] #2 Recovered builds are eligible for automatic retry and queue processing resumes without manual intervention.
- [ ] #3 Runtime builder disconnection/death causes in-flight `building` jobs to be re-queued instead of remaining stuck.
- [ ] #4 State transition reason is logged clearly for restart/runtime recovery paths.
- [ ] #5 No duplicate concurrent execution is introduced for a single build record during recovery.
- [ ] #6 Existing build queue behavior for normal successful builds remains unchanged.
- [ ] #7 Targeted integration tests cover startup orphan recovery and runtime builder-loss recovery.
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
LOCK: openai-gpt-5.5 on reckless in ~/code/crystal-forge/TASK-282-builder-stuck-building-recovery
<!-- SECTION:NOTES:END -->

## Definition of Done
<!-- DOD:BEGIN -->
- [ ] #1 Targeted queue recovery tests pass in Nix dev environment.
- [ ] #2 Manual verification demonstrates queue unblocks after simulated restart/disconnect.
<!-- DOD:END -->
