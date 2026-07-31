---
id: TASK-336.8
title: >-
  Admin Server: maintenance actions (backup DB / reload config / export audit /
  invalidate sessions)
status: Backlog
assignee: []
created_date: '2026-06-20 03:00'
labels:
  - admin
  - server
  - maintenance
  - api
  - backend
milestone: 'm-20: Design Parity Missing Surfaces'
dependencies: []
priority: low
ordinal: 314000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The Admin Server Maintenance card shows four actions: Backup database, Reload config, Export audit log, and Invalidate all sessions. These are currently disabled with not-implemented notices. Implement backend endpoints for each action and wire the buttons to real API behavior with appropriate confirmation and feedback.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Backup database triggers a real database backup and returns or streams the result
- [ ] #2 Reload config causes the server to reload its configuration without restart
- [ ] #3 Export audit log returns a downloadable audit log file
- [ ] #4 Invalidate all sessions revokes all active sessions with a confirmation step
- [ ] #5 Each action provides appropriate success/error feedback in the UI
<!-- AC:END -->
