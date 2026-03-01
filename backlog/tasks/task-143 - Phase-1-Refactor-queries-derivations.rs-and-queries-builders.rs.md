---
id: TASK-143
title: 'Phase 1: Refactor queries/derivations.rs and queries/builders.rs'
status: To Do
assignee: []
created_date: '2026-03-01 15:59'
labels:
  - refactoring
  - architecture
  - phase-1
milestone: m-2
dependencies:
  - TASK-3
priority: high
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
