---
id: TASK-3
title: 'Phase 2: Decompose builder/mod.rs God Object'
status: In Progress
assignee:
  - KimiK2.5
created_date: '2026-02-04 20:15'
updated_date: '2026-03-01 14:44'
labels:
  - refactoring
  - architecture
  - phase-2
milestone: m-2
dependencies:
  - TASK-2
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Break down 1094-line builder/mod.rs into focused, testable modules. Current file violates Single Responsibility Principle by handling build orchestration, CVE scanning, cache pushing, worker management, and error recovery.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Extract worker status management to status.rs
- [x] #2 Extract reservation cleanup to reservation.rs
- [x] #3 Extract CVE scan worker to cve_worker.rs
- [x] #4 Extract cache push worker to cache_worker.rs
- [x] #5 Extract build worker to worker.rs
- [x] #6 Create builder error types in error.rs
- [x] #7 Refactor mod.rs to orchestrate only
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Target: No file >300 lines, each module >80% coverage, all tests pass

LOCK: claude-sonnet-4-5 on gray in ~/code/crystal-forge/TASK-3-code-cleanup-refactoring

Implementation complete - builder/mod.rs refactored into focused submodules. All 7 acceptance criteria met. See commit 1771637 for details.
<!-- SECTION:NOTES:END -->
