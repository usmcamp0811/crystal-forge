---
id: TASK-3
title: 'Phase 2: Decompose builder/mod.rs God Object'
status: To Do
assignee: []
created_date: '2026-02-04 20:15'
labels:
  - refactoring
  - architecture
  - phase-2
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
- [ ] #1 Extract worker status management to status.rs
- [ ] #2 Extract reservation cleanup to reservation.rs
- [ ] #3 Extract CVE scan worker to cve_worker.rs
- [ ] #4 Extract cache push worker to cache_worker.rs
- [ ] #5 Extract build worker to worker.rs
- [ ] #6 Create builder error types in error.rs
- [ ] #7 Refactor mod.rs to orchestrate only
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Target: No file >300 lines, each module >80% coverage, all tests pass
<!-- SECTION:NOTES:END -->
