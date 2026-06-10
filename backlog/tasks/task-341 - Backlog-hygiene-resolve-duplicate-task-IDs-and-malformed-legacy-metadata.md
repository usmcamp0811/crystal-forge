---
id: TASK-341
title: 'Backlog hygiene: resolve duplicate task IDs and malformed legacy metadata'
status: Backlog
assignee: []
created_date: '2026-06-10 03:01'
labels:
  - backlog
  - maintenance
  - tooling
milestone: 'm-1: Development Infrastructure'
dependencies: []
references:
  - backlog/tasks
  - backlog/milestones
documentation:
  - design/doc-10 - CrystalForgelatest-parity-execution-plan.md
modified_files:
  - backlog/tasks/**
  - backlog/milestones/**
priority: medium
ordinal: 1720
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Problem: backlog maintenance surfaced historical data issues that the MCP tools cannot fully repair automatically, including duplicate active task IDs, malformed task metadata (for example stale TASK-303/TASK-215 branch conflicts), and duplicate milestone identifiers.

Desired Outcome: backlog metadata is normalized so task/milestone tools address the intended records unambiguously and future grooming is reliable.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Duplicate active task IDs are resolved or archived so MCP task operations are unambiguous
- [ ] #2 Malformed legacy task records are corrected or archived
- [ ] #3 Duplicate milestone identifiers/titles are normalized
- [ ] #4 Backlog task and milestone tooling can address the cleaned records without branch/conflict errors
<!-- AC:END -->
