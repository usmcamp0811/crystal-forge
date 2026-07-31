---
id: TASK-375.1
title: Stream live builder system load over WebSocket
status: Backlog
assignee: []
created_date: '2026-06-28 17:52'
labels:
  - builders
  - websocket
  - telemetry
milestone: Future
dependencies: []
references:
  - packages/default/src/builder/api_client.rs
  - packages/default/src/builder/metrics.rs
  - packages/default/src/handlers/api/builders.rs
  - packages/default/src/models/builders.rs
priority: medium
ordinal: 0
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Problem: Builder cards show system load, but API-mode builders currently report general CPU/memory load via periodic HTTP heartbeat rather than a live builder-load WebSocket stream. This makes UI load bars less timely and mixes live telemetry with heartbeat polling.

Desired Outcome: API-mode builders push live system load metrics (CPU and memory usage, plus active jobs if useful) to the server over WebSocket, with HTTP heartbeat remaining as a fallback/source of liveness. The UI builder card load bar can update promptly from server-side live telemetry.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Builder system load metrics are pushed over an authenticated WebSocket path in API mode.
- [ ] #2 Server records or broadcasts live builder load updates without relying only on heartbeat polling.
- [ ] #3 HTTP heartbeat remains available as a fallback for liveness/metrics.
- [ ] #4 UI builder card load data can be updated from the live telemetry path.
- [ ] #5 Tests cover the WebSocket load message shape and server handling.
<!-- AC:END -->
