---
id: TASK-349
title: Caches sidebar surface umbrella
status: Backlog
assignee: []
created_date: '2026-06-10 03:35'
labels:
  - design-parity
  - caches
  - umbrella
milestone: 'm-19: Design Parity Existing Surfaces'
dependencies: []
references:
  - TASK-303
  - TASK-341
  - /home/mcamp/code/crystal-forge/CrystalForgelatest/components/CachesView.jsx
  - design/doc-13 - Sidebar-surface-execution-map.md
documentation:
  - design/doc-13 - Sidebar-surface-execution-map.md
modified_files:
  - packages/web-ui/src/views/caches.rs
  - checks/web-ui/tests/integration-test.js
priority: high
ordinal: 1800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Problem: Caches work includes historical merged parity work plus stale malformed backlog metadata, making the user-facing Caches surface hard to read from backlog alone.

Desired Outcome: A single umbrella task exists for the Caches sidebar surface so any remaining discrepancy tasks and metadata cleanup can roll up into one surface record.

Scope: planning/coordination only. Direct implementation belongs in child discrepancy tasks or metadata-cleanup work.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A single umbrella record exists for the Caches sidebar surface
- [ ] #2 Remaining Caches discrepancy or metadata-cleanup tasks are identified and linked from this umbrella
- [ ] #3 The Caches surface is only considered complete when linked child tasks and verification evidence are complete
<!-- AC:END -->
