---
id: TASK-3.5
title: Extract BuildWorker to src/builder/worker.rs
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
Create new module for build worker. Move build_worker and build_task_description.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Create src/builder/worker.rs
- [ ] #2 Define BuildWorker struct
- [ ] #3 Implement claim_and_build_next() method
- [ ] #4 Implement run_worker_loop() background task
- [ ] #5 Move build_task_description logic
- [ ] #6 Add unit tests for build orchestration
- [ ] #7 Update mod.rs to use BuildWorker
<!-- AC:END -->
