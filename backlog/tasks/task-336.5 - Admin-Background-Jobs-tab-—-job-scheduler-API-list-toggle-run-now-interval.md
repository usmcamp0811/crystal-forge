---
id: TASK-336.5
title: 'Admin: Background Jobs tab — job scheduler API (list/toggle/run-now/interval)'
status: Backlog
assignee: []
created_date: '2026-06-20 02:59'
labels:
  - admin
  - jobs
  - api
  - backend
milestone: 'm-20: Design Parity Missing Surfaces'
dependencies: []
parent_task_id: TASK-336
priority: low
ordinal: 311000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The Admin Background Jobs tab shows scheduled server-side tasks with status, interval, load, last-run, next-run, enable/disable toggle, and run-now action. No backend API exposes this yet. Add the jobs API and wire the tab to real data.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A backend API exposes scheduled job list with status, interval, load, last-run, next-run, and enabled state
- [ ] #2 The Background Jobs tab can toggle enabled state and trigger run-now via real API calls
- [ ] #3 The tab renders live job data instead of the not-implemented placeholder
<!-- AC:END -->
