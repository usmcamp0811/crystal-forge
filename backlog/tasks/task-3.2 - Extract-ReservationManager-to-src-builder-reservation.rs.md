---
id: TASK-3.2
title: Extract ReservationManager to src/builder/reservation.rs
status: Backlog
assignee: []
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
Create new module for reservation cleanup. Move run_reservation_cleanup_loop.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Create src/builder/reservation.rs
- [ ] #2 Define ReservationManager struct
- [ ] #3 Implement cleanup_stale_reservations() method
- [ ] #4 Implement run_cleanup_loop() background task
- [ ] #5 Add unit tests for cleanup logic
- [ ] #6 Update mod.rs to use ReservationManager
<!-- AC:END -->
