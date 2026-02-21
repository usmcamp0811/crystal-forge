---
id: TASK-3.7
title: Refactor src/builder/mod.rs to orchestration only
status: Backlog
assignee: ["KimiK2.5"]
created_date: '2026-02-04 21:12'
updated_date: '2026-02-19 03:39'
labels:
  - refactoring
  - builder
  - rust
milestone: m-2
dependencies: []
parent_task_id: TASK-3
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Reduce mod.rs to orchestration layer only, delegating to worker modules.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Keep only run_build_loop(), run_cve_scan_loop(), run_cache_push_loop()
- [ ] #2 Remove all implementation details
- [ ] #3 Delegate to worker structs
- [ ] #4 Verify mod.rs < 100 lines
- [ ] #5 Ensure all integration tests pass
<!-- AC:END -->
