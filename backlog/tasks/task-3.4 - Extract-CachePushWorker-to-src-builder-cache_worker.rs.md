---
id: TASK-3.4
title: Extract CachePushWorker to src/builder/cache_worker.rs
status: Backlog
assignee: []
created_date: '2026-02-04 21:12'
updated_date: '2026-02-19 03:39'
labels:
  - refactoring
  - builder
  - cache
  - rust
milestone: m-2
dependencies: []
parent_task_id: TASK-3
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Create new module for cache pushing. Consolidate duplicate implementations.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Create src/builder/cache_worker.rs
- [ ] #2 Define CachePushWorker struct
- [ ] #3 Consolidate run_cache_push_loop and run_cache_push_workers
- [ ] #4 Implement push_next_job() method
- [ ] #5 Implement run_push_loop() for S3/Attic/Nix
- [ ] #6 Add unit tests for push logic
- [ ] #7 Update mod.rs to use CachePushWorker
<!-- AC:END -->
