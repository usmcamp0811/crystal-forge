---
id: TASK-342
title: Dashboard sidebar surface umbrella
status: Backlog
assignee: []
created_date: '2026-06-10 03:23'
updated_date: '2026-06-10 03:36'
labels:
  - design-parity
  - dashboard
  - web-ui
milestone: 'm-19: Design Parity Existing Surfaces'
dependencies: []
references:
  - >-
    /home/mcamp/code/crystal-forge/CrystalForgelatest/components/DashboardView.jsx
  - TASK-321
  - TASK-341
  - design/doc-13 - Sidebar-surface-execution-map.md
documentation:
  - design/doc-10 - CrystalForgelatest-parity-execution-plan.md
  - design/doc-11 - CrystalForgelatest-design-source-index.md
  - design/doc-13 - Sidebar-surface-execution-map.md
modified_files:
  - packages/web-ui/src/views/dashboard.rs
  - checks/web-ui/tests/integration-test.js
priority: high
ordinal: 1730
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Problem: the Dashboard surface already has substantial completed parity work, but backlog cleanup exposed duplicate/malformed dashboard task history that makes the remaining user-facing dashboard state hard to read from one place.

Desired Outcome: this task serves as the single umbrella record for the Dashboard sidebar surface and points to any remaining discrepancy work relative to CrystalForgelatest.

Scope: planning/coordination only. Direct implementation should happen in detailed discrepancy tasks.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A clean non-duplicated umbrella record exists for the Dashboard sidebar surface
- [ ] #2 Remaining dashboard deltas are explicitly scoped relative to TASK-321 rather than rediscovering completed work
- [ ] #3 Dashboard screenshot/assertion expectations are aligned with the parity docs if further work is required
<!-- AC:END -->
