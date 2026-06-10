---
id: TASK-342
title: Close remaining Dashboard parity gaps against CrystalForgelatest
status: Backlog
assignee: []
created_date: '2026-06-10 03:23'
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
documentation:
  - design/doc-10 - CrystalForgelatest-parity-execution-plan.md
  - design/doc-11 - CrystalForgelatest-design-source-index.md
modified_files:
  - packages/web-ui/src/views/dashboard.rs
  - checks/web-ui/tests/integration-test.js
priority: high
ordinal: 1730
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Problem: backlog cleanup surfaced an ambiguous duplicate TASK-327 state around Dashboard parity work. The codebase already has substantial Dashboard parity implementation from TASK-321, but any remaining dashboard-specific parity work needs a clean, non-duplicated task record.

Desired Outcome: a single clean task exists to track only the remaining Dashboard parity deltas, if any, against `/home/mcamp/code/crystal-forge/CrystalForgelatest/components/DashboardView.jsx`, without relying on the malformed duplicate TASK-327 metadata.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A clean non-duplicated backlog record exists for any remaining Dashboard parity work
- [ ] #2 Remaining dashboard deltas are explicitly scoped relative to TASK-321 rather than rediscovering completed work
- [ ] #3 Dashboard screenshot/assertion expectations are aligned with the parity docs if further work is required
<!-- AC:END -->
