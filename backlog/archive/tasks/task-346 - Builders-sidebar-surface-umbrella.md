---
id: TASK-346
title: Builders sidebar surface umbrella
status: Backlog
assignee: []
created_date: '2026-06-10 03:35'
labels:
  - builders
  - umbrella
  - planning
milestone: 'm-0: Critical Bugs & Stability'
dependencies: []
references:
  - TASK-204
  - TASK-291
  - >-
    /home/mcamp/code/crystal-forge/CrystalForgelatest/components/BuildersView.jsx
  - design/doc-13 - Sidebar-surface-execution-map.md
documentation:
  - design/doc-13 - Sidebar-surface-execution-map.md
modified_files:
  - packages/web-ui/src/views/builders.rs
  - checks/web-ui/tests/integration-test.js
priority: high
ordinal: 1790
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Problem: Builders-related user-facing work is split across RBAC/view bugs and runtime/configuration issues, making it hard to track the Builders navigation surface as one thing.

Desired Outcome: A single umbrella task exists for the Builders sidebar surface so UI access bugs, configuration behavior, and parity/verification work can roll up into one surface record.

Scope: planning/coordination only. Direct implementation belongs in child discrepancy tasks.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A single umbrella record exists for the Builders sidebar surface
- [ ] #2 Remaining Builders discrepancy tasks are identified and linked from this umbrella
- [ ] #3 The Builders surface is only considered complete when linked child tasks and verification evidence are complete
<!-- AC:END -->
