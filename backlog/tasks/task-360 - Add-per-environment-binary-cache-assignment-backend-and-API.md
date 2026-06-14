---
id: TASK-360
title: Add per-environment binary cache assignment backend and API
status: Backlog
assignee: []
created_date: '2026-06-14 19:10'
labels:
  - environments
  - caches
  - backend
  - design-parity-followup
milestone: 'm-19: Design Parity Existing Surfaces'
dependencies: []
priority: medium
ordinal: 303000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem
The Environments design reference shows a binary cache assigned to each environment (with cache type, status, storage usage, paths) and a cache picker in the Add/Edit modal. The backend has no per-environment cache assignment — TASK-358 renders cache fields from clearly-commented placeholders.

## Desired Outcome
Add backend schema (new migration), query, and API support to assign a cache destination to an environment and return the assignment, then wire the Environments UI (TASK-358) and the Add/Edit modal cache picker to authoritative data. Requires cross-reference to the Caches/cache_destinations data.
<!-- SECTION:DESCRIPTION:END -->
