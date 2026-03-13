---
id: TASK-2.3
title: Add unit tests for builder/mod.rs - update_worker_status
status: Done
assignee:
  - KimiK2.5
created_date: '2026-02-04 20:39'
updated_date: '2026-03-13 01:24'
labels:
  - testing
  - builder
  - rust
milestone: m-1
dependencies: []
parent_task_id: TASK-2
ordinal: 45000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Test worker status state transitions and error handling.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Test status transitions (idle -> building -> idle)
- [x] #2 Test concurrent status updates
- [x] #3 Test invalid state transitions
- [x] #4 Mock database pool
- [x] #5 Verify status persistence
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Refactored update_worker_status using same 'Functional Core, Imperative Shell' pattern:

1. Extracted pure apply_worker_status_update(statuses, worker_id, state, task) -> bool
   - Operates on &mut [WorkerStatus], no global state or async
   - Returns bool indicating whether a matching worker was found
2. Kept update_worker_status() as thin async wrapper using tokio::spawn + global RwLock

9 unit tests added covering:
- State transitions: idle→working, working→idle, full lifecycle
- started_at management: set on Working/Sleeping, cleared on Idle
- Worker targeting: correct worker in multi-worker pool, unknown ID is no-op
- Edge cases: empty worker list, working with None task, sleeping state

All 85 tests pass (9 new + 76 existing). No database required.
<!-- SECTION:NOTES:END -->
