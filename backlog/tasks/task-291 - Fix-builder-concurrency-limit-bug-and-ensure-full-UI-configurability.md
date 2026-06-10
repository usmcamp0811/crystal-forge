---
id: TASK-291
title: Fix builder concurrency limit bug and ensure full UI configurability
status: Backlog
assignee: []
created_date: '2026-05-08 02:46'
updated_date: '2026-06-10 02:59'
labels:
  - bug
  - builders
  - ui
  - configuration
  - high-priority
milestone: 'm-0: Critical Bugs & Stability'
dependencies: []
references:
  - packages/default/src/bin/builder.rs
  - packages/web-ui/src/views/builders.rs
modified_files:
  - packages/default/src/bin/builder.rs
  - packages/web-ui/src/views/builders.rs
priority: high
ordinal: 15000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

The builder is using the wrong configuration value for its concurrency semaphore, causing builders to potentially stall when `build.max_concurrent_derivations` is set to 0 or an incorrect value.

## Goal

1. Fix the immediate bug: use `builder_config.max_concurrent_jobs` instead of `build_config.max_concurrent_derivations` in the builder semaphore path.
2. Verify builders actually respect database-backed concurrency updates end-to-end.
3. Confirm UI-driven builder settings remain the authoritative runtime source after bootstrap.

## Replan note
This task had partially completed implementation when work stalled. It is reset to Backlog so the remaining end-to-end runtime verification can be re-scoped and resumed cleanly.

## Scope
- Preserve the landed code fix and UI audit work already recorded in this task.
- Finish runtime verification for concurrency behavior and DB-over-config precedence.
- Add any missing tests needed to prove the behavior conclusively.
<!-- SECTION:DESCRIPTION:END -->

<!-- AC:END -->

<!-- AC:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Line 225 in builder.rs uses builder_config.max_concurrent_jobs.unwrap_or(1) instead of build_config.max_concurrent_derivations
- [x] #2 Comment added explaining the distinction between builder_config.max_concurrent_jobs (builder-level concurrency) and build_config.max_concurrent_derivations (Nix-level parallelism)
- [ ] #3 Builder with max_concurrent_jobs = 2 successfully claims and processes up to 2 jobs concurrently
- [ ] #4 Builder with max_concurrent_jobs = 1 successfully claims and processes only 1 job at a time
- [ ] #5 Builder respects the max_concurrent_jobs value from the database (not config file) if they differ
- [x] #6 UI builder management view exposes all configurable builder fields: name, status, max_concurrent_jobs, max_cpu_cores, max_memory_mb, environment assignments
- [x] #7 UI validates that max_concurrent_jobs must be >= 1
- [ ] #8 Changes to max_concurrent_jobs made via UI are persisted to database and reflected in builder behavior
- [ ] #9 Builder either immediately respects updated max_concurrent_jobs or picks up changes on next heartbeat cycle
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Reset to Backlog during cleanup. Resume from the partially landed fix rather than restarting discovery.
<!-- SECTION:NOTES:END -->
