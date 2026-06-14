---
id: TASK-359
title: Persist per-environment deployment policy and production flag
status: Backlog
assignee: []
created_date: '2026-06-14 19:10'
labels:
  - environments
  - backend
  - design-parity-followup
milestone: 'm-19: Design Parity Existing Surfaces'
dependencies: []
priority: medium
ordinal: 302000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem
The Environments design reference shows a per-environment default deployment mode (manual / auto_latest / pinned) and a Production flag that gates destructive actions. The backend currently has no schema or API for these — TASK-358 renders them from clearly-commented placeholders.

## Desired Outcome
Add backend schema (new migration), query, and API support to persist and return each environment's default deployment policy and an is_production boolean, and wire the Environments UI (TASK-358 work) to authoritative data instead of placeholders.
<!-- SECTION:DESCRIPTION:END -->
