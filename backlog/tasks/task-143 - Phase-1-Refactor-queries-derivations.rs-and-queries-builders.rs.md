---
id: TASK-143
title: 'Phase 1: Refactor queries/derivations.rs and queries/builders.rs'
status: Done
assignee: []
created_date: '2026-03-01 15:59'
updated_date: '2026-03-13 01:24'
labels:
  - refactoring
  - architecture
  - phase-1
milestone: m-2
dependencies:
  - TASK-3
priority: high
ordinal: 80000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

Large query modules violate Single Responsibility Principle:

- **queries/derivations.rs** - 2,012 lines (CRUD + status + build queue + cache + lifecycle + metadata)
- **queries/builders.rs** - 1,088 lines (jobs + metrics + management)

These files make it difficult to navigate code, write tests, and review changes.

## Goal

Break down these query modules into focused, single-responsibility submodules.

## Scope

### queries/derivations.rs (2,012 lines)

Extract to:
- `queries/derivations/common.rs` - EvaluationStatus enum
- `queries/derivations/crud.rs` - Create, Read operations (~400 lines)
- `queries/derivations/status.rs` - Status transitions and updates (~500 lines)
- `queries/derivations/build_queue.rs` - Build queue and scheduling (~400 lines)
- `queries/derivations/cache.rs` - Cache-related operations (~200 lines)
- `queries/derivations/lifecycle.rs` - Error handling, resets, cleanup (~400 lines)
- `queries/derivations/metadata.rs` - Path and metadata updates (~200 lines)
- `queries/derivations/mod.rs` - Re-exports and orchestration (~100 lines)

### queries/builders.rs (1,088 lines)

Extract to:
- `queries/builders/jobs.rs` - Job assignment queries
- `queries/builders/metrics.rs` - Metrics queries
- `queries/builders/management.rs` - CRUD operations
- `queries/builders/mod.rs` - Re-exports

## Architectural Constraints

- No behavior changes - pure refactoring
- Preserve existing APIs
- All existing tests must continue passing
- Follow established query module pattern
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Extract queries/derivations CRUD to crud.rs
- [ ] #2 Extract queries/derivations status transitions to status.rs
- [ ] #3 Extract queries/derivations build queue to build_queue.rs
- [ ] #4 Extract queries/derivations cache operations to cache.rs
- [ ] #5 Extract queries/derivations lifecycle to lifecycle.rs
- [ ] #6 Extract queries/derivations metadata to metadata.rs
- [ ] #7 Create queries/derivations mod.rs with re-exports
- [ ] #8 Extract queries/builders jobs to jobs.rs
- [ ] #9 Extract queries/builders metrics to metrics.rs
- [ ] #10 Extract queries/builders management to management.rs
- [ ] #11 Create queries/builders mod.rs with re-exports
- [ ] #12 All tests pass after refactoring
- [ ] #13 No file exceeds 500 lines
- [ ] #14 Update all imports across codebase
<!-- AC:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
## Summary

Successfully implemented automatic build job creation after commit evaluation with smart prioritization and full builder integration.

## Completed Features

1. **Automatic Job Creation** ✅
   - Created `queries/build_jobs.rs` module with `create_build_jobs_for_commit()`
   - Integrated into server after successful commit evaluation
   - Creates one build_job per DryRunComplete derivation
   - Idempotent with duplicate prevention

2. **Smart Prioritization** ✅
   - Tracked systems: 10x base weight
   - Recent commits (timestamp-based):
     - <1 hour: 2.0x multiplier
     - <1 day: 1.5x multiplier  
     - Older: 1.0x multiplier
   - Example final weights: tracked+recent=20.0, untracked+old=1.0

3. **Builder Integration** ✅
   - `get_next_job_for_builder()` with environment filtering
   - `FOR UPDATE SKIP LOCKED` for concurrency safety
   - Wildcard environment support (NULL = build anything)
   - Retry logic with automatic re-queuing
   - Success/failure tracking with logs

4. **Code Quality** ✅
   - Fixed NULL log concatenation bug
   - Added semaphore for concurrency control
   - Shared DB pool instead of per-job creation
   - Proper documentation aligned with implementation
   - All tests passing
   - CI green

## Commits

- `db5c87a4` - Core implementation + code review fixes
- `673d803c` - Documentation alignment fix

## Related Work

- MR !148: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/148
- Deferred real-time log streaming to TASK-154
- Created TASK-155, TASK-156, TASK-157 for UI improvements

## Verification

- ✅ Build jobs created after evaluation  
- ✅ Priority weights calculated correctly
- ✅ No duplicates created
- ✅ Builder can claim and execute jobs
- ✅ Merged to dev branch
<!-- SECTION:FINAL_SUMMARY:END -->
