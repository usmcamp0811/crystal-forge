---
id: TASK-357.2
title: Persist flake auto-sync settings and interval
status: Backlog
assignee: []
created_date: '2026-06-15 01:24'
labels:
  - flakes
  - backend
  - design-parity
milestone: 'm-19: Design Parity Existing Surfaces'
dependencies: []
references:
  - TASK-357
parent_task_id: TASK-357
priority: medium
ordinal: 0
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Problem: The Flakes reference modal includes auto-sync and sync interval controls, but the current flake CRUD API does not persist those settings. Enabling editable UI controls would imply state is saved when it is not.

Desired Outcome: Add backend persistence and API fields for per-flake auto-sync enablement and sync interval so the Add/Edit flake modal can save and reload those controls authoritatively.
<!-- SECTION:DESCRIPTION:END -->
