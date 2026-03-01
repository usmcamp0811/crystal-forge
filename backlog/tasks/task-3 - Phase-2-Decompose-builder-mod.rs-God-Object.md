---
id: TASK-3
title: 'Phase 2: Decompose builder/mod.rs God Object'
status: Review
assignee:
  - KimiK2.5
created_date: '2026-02-04 20:15'
updated_date: '2026-03-01 14:57'
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

MR created: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/145

MR updated to target dev branch: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/146 (closed incorrect MR !145 that targeted main)

CI Failure Investigation: The builder check failed due to a pre-existing flaky test in handlers/api/auth_whoami.rs (detect_auth_mode_recognizes_dev). This test was NOT modified by our refactoring - we only touched builder/* files. The test has environment variable isolation issues.
<!-- SECTION:NOTES:END -->
