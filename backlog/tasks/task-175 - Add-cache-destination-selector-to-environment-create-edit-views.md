---
id: TASK-175
title: Add cache destination selector to environment create/edit views
status: Backlog
assignee: []
created_date: '2026-03-08 17:14'
labels:
  - ui
  - environments
  - cache
dependencies: []
references:
  - packages/web-ui/src/views/environments.rs
  - packages/web-ui/src/components/environments/
  - packages/default/src/handlers/api/environments.rs
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Problem: Environment creation/editing currently has no way to choose which cache destination should be used for that environment. With cache destinations now managed in the UI, operators need an explicit per-environment cache selection to avoid relying on implicit global defaults.

Desired outcome: In the Add/Edit Environment UI, provide a cache destination selector populated from configured cache destinations, persist the selected cache on the environment model, and surface it in environment details/lists as appropriate.

Notes:
- Reuse existing cache destinations API.
- Include validation for missing/disabled destinations.
- Keep backward compatibility for environments without explicit cache assignment.
<!-- SECTION:DESCRIPTION:END -->
