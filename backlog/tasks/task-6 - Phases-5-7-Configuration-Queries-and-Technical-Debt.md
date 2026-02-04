---
id: TASK-6
title: 'Phases 5-7: Configuration, Queries, and Technical Debt'
status: To Do
assignee: []
created_date: '2026-02-04 20:16'
labels:
  - refactoring
  - cleanup
  - phase-5
  - phase-6
  - phase-7
dependencies:
  - TASK-5
priority: low
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Polish and cleanup: configuration refactoring, query organization, technical debt resolution. Includes consolidating config getters, standardizing query patterns, and resolving 8 TODO comments.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Consolidate configuration getters
- [ ] #2 Separate validation logic
- [ ] #3 Create configuration builder
- [ ] #4 Standardize query patterns (all use SQLx macros)
- [ ] #5 Add transaction helpers
- [ ] #6 Replace deriver_drv with resolve_store_path
- [ ] #7 Implement initial commit fetching
- [ ] #8 Update systems table schema
- [ ] #9 Add server name to build metadata
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Target: CrystalForgeConfig <150 lines, all queries use SQLx macros, all TODOs resolved
<!-- SECTION:NOTES:END -->
