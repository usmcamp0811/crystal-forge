---
id: TASK-282
title: Fix builder stuck in "building" state after server restart
status: Review
assignee:
  - '@openai-gpt-5.5'
created_date: '2026-04-20 19:22'
updated_date: '2026-06-27 03:05'
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
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/287'
modified_files:
  - packages/default/src/queries/builders.rs
  - packages/default/src/server/mod.rs
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
- [x] #1 On server startup, any orphaned build rows in `building` with no active builder execution are transitioned to queued/pending.
- [x] #2 Recovered builds are eligible for automatic retry and queue processing resumes without manual intervention.
- [x] #3 Runtime builder disconnection/death causes in-flight `building` jobs to be re-queued instead of remaining stuck.
- [x] #4 State transition reason is logged clearly for restart/runtime recovery paths.
- [x] #5 No duplicate concurrent execution is introduced for a single build record during recovery.
- [x] #6 Existing build queue behavior for normal successful builds remains unchanged.
- [x] #7 Targeted integration tests cover startup orphan recovery and runtime builder-loss recovery.
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Review findings addressed and pushed in commit `668e90df`.

Fixes:
- Disabled builders are now treated as invalid owners during orphan recovery: jobs owned by builders with `status = 'active'` and `enabled = false` are requeued.
- Recovery log append now preserves the 10 MiB build log ceiling by truncating existing logs before appending the recovery reason.
- Added DB-backed tests for the disabled-builder orphan case and recovery log truncation.

Verification rerun:
- `test_requeue_orphaned_building_jobs_treats_disabled_active_builder_as_orphaned` ✅
- `test_requeue_orphaned_building_jobs_preserves_log_size_limit` ✅
- original startup/runtime/stale race recovery tests ✅
- `nix develop -c rustfmt --edition 2024 --check packages/default/src/queries/builders.rs packages/default/src/server/mod.rs` ✅
- `nix develop -c env SQLX_OFFLINE=true cargo check --manifest-path packages/default/Cargo.toml` ✅
- `nix build .#packages.x86_64-linux.server --no-link` ✅

GitLab pipeline #2633355744 is running for MR !287.
<!-- SECTION:NOTES:END -->

## Definition of Done
<!-- DOD:BEGIN -->
- [x] #1 Targeted queue recovery tests pass in Nix dev environment.
- [x] #2 Manual verification demonstrates queue unblocks after simulated restart/disconnect.
<!-- DOD:END -->
