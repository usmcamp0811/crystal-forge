---
id: TASK-362
title: 'Add per-environment auto-sync, approval, and RBAC role assignment backend'
status: Backlog
assignee: []
created_date: '2026-06-14 19:10'
labels:
  - environments
  - rbac
  - backend
  - design-parity-followup
milestone: 'm-19: Design Parity Existing Surfaces'
dependencies: []
priority: medium
ordinal: 305000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem
The Environments design reference shows per-environment auto-sync toggle, requires-approval toggle, and an RBAC role-assignment summary (admin/operator/viewer counts). The backend has no schema or API for these — TASK-358 renders them from clearly-commented placeholders.

## Desired Outcome
Add backend schema (new migration), queries, and API to persist per-environment auto-sync, requires-approval, and RBAC role assignments, returning them in the environment payload. Wire the Environments UI and Add/Edit modal (TASK-358) to authoritative data instead of placeholders.
<!-- SECTION:DESCRIPTION:END -->
