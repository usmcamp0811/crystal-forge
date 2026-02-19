---
id: TASK-5
title: 'Phase 4: Improve Error Handling'
status: Backlog
assignee: ["Codex 5.3"]
created_date: '2026-02-04 20:16'
updated_date: '2026-02-19 03:39'
labels:
  - refactoring
  - error-handling
  - phase-4
milestone: m-2
dependencies:
  - TASK-4
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Implement consistent, structured error handling across application. Currently mix of anyhow::Result, Result<T, StatusCode>, and panics with inconsistent error context.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Define CrystalForgeError enum with domain-specific variants
- [ ] #2 Implement error conversion traits
- [ ] #3 Update services for structured errors
- [ ] #4 Update handlers for structured errors
- [ ] #5 Add error logging with context
- [ ] #6 Update documentation
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Target: No anyhow::Error in public APIs, all errors have clear messages
<!-- SECTION:NOTES:END -->
