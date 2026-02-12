---
id: TASK-3.1
title: Extract WorkerStatusManager to src/builder/status.rs
status: To Do
assignee: []
created_date: '2026-02-04 21:12'
labels:
  - refactoring
  - builder
  - rust
dependencies: []
parent_task_id: TASK-3
milestone: m-2
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Create new module for worker status tracking. Move update_worker_status and related logic.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Create src/builder/status.rs
- [ ] #2 Define WorkerStatusManager struct
- [ ] #3 Implement new(), update_status(), get_status() methods
- [ ] #4 Move update_worker_status logic
- [ ] #5 Add unit tests for status transitions
- [ ] #6 Update mod.rs to use WorkerStatusManager
<!-- AC:END -->
