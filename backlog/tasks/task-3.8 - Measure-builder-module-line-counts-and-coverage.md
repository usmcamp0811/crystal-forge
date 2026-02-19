---
id: TASK-3.8
title: Measure builder module line counts and coverage
status: Backlog
assignee: ["KimiK2.5"]
created_date: '2026-02-04 21:12'
updated_date: '2026-02-19 03:39'
labels:
  - metrics
  - testing
  - builder
milestone: m-2
dependencies: []
parent_task_id: TASK-3
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Verify no file >300 lines and each module >80% coverage.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Run tokei on src/builder/
- [ ] #2 Verify no file >300 lines
- [ ] #3 Run cargo tarpaulin on builder module
- [ ] #4 Verify each module >80% coverage
- [ ] #5 Document metrics in PR
<!-- AC:END -->
