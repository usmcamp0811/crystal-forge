---
id: TASK-496
title: >-
  Periodic agent heartbeats fall back to full states after a database comparison
  error
status: Backlog
assignee: []
created_date: '2026-10-19 05:37'
labels: []
dependencies: []
references:
  - packages/default/crates/cf-server/src/handlers/agent/heartbeat.rs
  - packages/default/crates/cf-server/src/queries/system_states.rs
  - checks/integration-delivery/scenarios/delivery-lifecycle.py
  - /tmp/opencode/delivery-local-run6.log
priority: medium
type: bug
ordinal: 547000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The real target agent registers and sends signed periodic heartbeat requests, but the server repeatedly logs 'Heartbeat became state change: Database error occurred while checking state' and persists a new full state. Observed in the five-role Delivery lifecycle based on e07b4bd2. The agent journal records periodic heartbeat sends; the controller accepts each request. Registration and periodic reporting work, but lightweight heartbeat classification is not proven correct. This product defect is outside the no-production-changes test overhaul.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 An unchanged registered target's periodic signed heartbeat is classified without a database comparison error.
- [ ] #2 Persisted heartbeat and state identities remain correlated to the exact registered target and running generation.
- [ ] #3 A genuine state change still creates the required full-state evidence rather than being misclassified as an unchanged heartbeat.
- [ ] #4 An integration or isolated-database regression and affected source documentation describe and verify the intended classification and failure behavior.
<!-- AC:END -->
