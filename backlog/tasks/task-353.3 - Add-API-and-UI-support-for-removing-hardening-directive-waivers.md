---
id: TASK-353.3
title: Add API and UI support for removing hardening directive waivers
status: Backlog
assignee: []
created_date: '2026-06-14 14:35'
labels:
  - systems
  - system-detail
  - hardening
  - web-ui
  - api
milestone: 'm-19: Design Parity Existing Surfaces'
dependencies: []
references:
  - packages/default/src/queries/hardening_scans.rs
  - packages/default/src/bin/server.rs
  - packages/web-ui/src/views/system_detail.rs
parent_task_id: TASK-353
priority: medium
ordinal: 0
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem
The System Detail Hardening modal design includes a Remove action for directive waivers, and the query layer already has hardening justification deletion support, but there is no routed backend API/client path exposed for the web UI to remove hardening justifications.

## Desired Outcome
Users can remove an existing hardening directive waiver from the Hardening detail modal through an authoritative backend API, with the UI refreshing waiver state after successful removal.
<!-- SECTION:DESCRIPTION:END -->
