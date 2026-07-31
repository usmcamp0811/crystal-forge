---
id: TASK-343
title: Flakes sidebar surface umbrella
status: Backlog
assignee: []
created_date: '2026-06-10 03:35'
labels:
  - design-parity
  - flakes
  - umbrella
milestone: 'm-19: Design Parity Existing Surfaces'
dependencies: []
references:
  - TASK-331
  - TASK-297.1
  - /home/mcamp/code/crystal-forge/CrystalForgelatest/components/FlakesView.jsx
  - design/doc-13 - Sidebar-surface-execution-map.md
documentation:
  - design/doc-10 - CrystalForgelatest-parity-execution-plan.md
  - design/doc-13 - Sidebar-surface-execution-map.md
modified_files:
  - packages/web-ui/src/views/flakes_list.rs
  - checks/web-ui/tests/integration-test.js
priority: high
ordinal: 1740
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Problem: Flakes-related work is currently split across parity, cleanup, and historical implementation tasks, making it hard to see the full surface state in one place.

Desired Outcome: A single sidebar-surface umbrella task tracks the Flakes view from the user perspective and points to the detailed discrepancy tasks required to bring the surface fully in line with the current design and behavior expectations.

Scope: planning/coordination only. Use implementation tasks for specific fixes, parity gaps, and verification work.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A single umbrella record exists for the Flakes sidebar surface
- [ ] #2 Remaining Flakes discrepancy tasks are identified and linked from this umbrella
- [ ] #3 The Flakes surface is only considered complete when linked child tasks and verification evidence are complete
<!-- AC:END -->
