---
id: TASK-336.7
title: 'Admin Server: classification banner config API (enable/level/custom text)'
status: Backlog
assignee: []
created_date: '2026-06-20 02:59'
labels:
  - admin
  - server
  - classification
  - api
  - backend
milestone: 'm-20: Design Parity Missing Surfaces'
dependencies: []
parent_task_id: TASK-336
priority: low
ordinal: 313000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The Admin Server tab Classification banners card allows enabling/disabling DoD/CNSS classification markings, selecting a classification level, and entering custom marking text. No backend API persists this config. Add the classification config endpoint and wire the toggle and fields to real behavior.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Backend exposes GET and PUT for classification config (enabled, level, custom text)
- [ ] #2 The Admin Server Classification banners card reads and saves configuration via the real API
- [ ] #3 Classification banner renders at top and bottom of the UI when enabled
<!-- AC:END -->
