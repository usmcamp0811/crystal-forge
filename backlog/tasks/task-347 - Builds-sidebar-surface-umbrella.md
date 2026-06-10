---
id: TASK-347
title: Builds sidebar surface umbrella
status: Backlog
assignee: []
created_date: '2026-06-10 03:35'
labels:
  - design-parity
  - builds
  - umbrella
milestone: 'm-19: Design Parity Existing Surfaces'
dependencies: []
references:
  - TASK-275
  - /home/mcamp/code/crystal-forge/CrystalForgelatest/components/BuildsView.jsx
  - design/doc-13 - Sidebar-surface-execution-map.md
documentation:
  - design/doc-10 - CrystalForgelatest-parity-execution-plan.md
  - design/doc-13 - Sidebar-surface-execution-map.md
modified_files:
  - packages/web-ui/src/views/builds.rs
  - checks/web-ui/tests/integration-test.js
priority: high
ordinal: 1750
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Problem: Builds work is split across historical refresh tasks and ongoing parity/verification work, making it hard to track the whole Builds surface as one navigation item.

Desired Outcome: A single umbrella task exists for the Builds sidebar surface so detailed discrepancy tasks can roll up into one easy-to-read surface record.

Scope: planning/coordination only. Direct implementation belongs in child discrepancy tasks.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A single umbrella record exists for the Builds sidebar surface
- [ ] #2 Remaining Builds discrepancy tasks are identified and linked from this umbrella
- [ ] #3 The Builds surface is only considered complete when linked child tasks and verification evidence are complete
<!-- AC:END -->
