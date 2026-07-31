---
id: TASK-336.6
title: >-
  Admin Server: agent heartbeat config API (global interval + per-environment
  overrides)
status: Backlog
assignee: []
created_date: '2026-06-20 02:59'
labels:
  - admin
  - server
  - heartbeat
  - api
  - backend
milestone: 'm-20: Design Parity Missing Surfaces'
dependencies: []
priority: medium
ordinal: 312000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The Admin Server tab Heartbeat card shows a global heartbeat interval, stale/offline multipliers, and per-environment overrides. No backend API exposes or persists this config yet. Add the heartbeat config endpoint and wire the card controls to real read/save behavior.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Backend exposes GET and PUT for heartbeat config (global interval, stale multiplier, offline multiplier, per-environment overrides)
- [ ] #2 The Admin Server Heartbeat card reads and saves configuration via the real API
<!-- AC:END -->
