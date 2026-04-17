---
id: TASK-227
title: Add 'Move to top' action on Active Eval Queue cards
status: Backlog
assignee: []
created_date: '2026-03-30 02:35'
labels:
  - ui
  - evaluation-queue
  - operator-ux
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem
Operators currently cannot quickly reprioritize a specific commit directly from Active Eval Queue cards.

## Desired Outcome
Add a per-card action (e.g., "Move to top") that reorders the selected commit to the front of the active evaluation queue using existing reorder APIs and permissions.

## Notes
- Respect RBAC (operator/admin only for reorder actions).
- Keep in-progress item semantics intact.
- Provide optimistic UI feedback and error handling when reorder fails.
<!-- SECTION:DESCRIPTION:END -->
