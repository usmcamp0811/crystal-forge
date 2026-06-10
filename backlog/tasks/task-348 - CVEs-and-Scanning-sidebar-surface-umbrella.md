---
id: TASK-348
title: CVEs and Scanning sidebar surface umbrella
status: Backlog
assignee: []
created_date: '2026-06-10 03:35'
labels:
  - design-parity
  - cves
  - scanning
  - umbrella
milestone: 'm-19: Design Parity Existing Surfaces'
dependencies: []
references:
  - TASK-326
  - TASK-327
  - TASK-331
  - /home/mcamp/code/crystal-forge/CrystalForgelatest/components/CvesView.jsx
  - >-
    /home/mcamp/code/crystal-forge/CrystalForgelatest/components/ScanningView.jsx
  - design/doc-13 - Sidebar-surface-execution-map.md
documentation:
  - design/doc-10 - CrystalForgelatest-parity-execution-plan.md
  - design/doc-13 - Sidebar-surface-execution-map.md
modified_files:
  - packages/web-ui/src/views/cves.rs
  - packages/web-ui/src/views/scanning.rs
  - checks/web-ui/tests/integration-test.js
priority: high
ordinal: 1770
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Problem: CVEs and Scanning are conceptually adjacent user-facing surfaces, but backlog work is split between completed scanning introduction, worker/backend policy tasks, and remaining parity work.

Desired Outcome: A single umbrella task exists for the CVEs/Scanning portion of the navigation so discrepancy tasks can be grouped under a user-facing surface record.

Scope: planning/coordination only. Direct implementation belongs in child discrepancy tasks.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A single umbrella record exists for the CVEs and Scanning sidebar surfaces
- [ ] #2 Remaining CVE/Scanning discrepancy tasks are identified and linked from this umbrella
- [ ] #3 The CVEs/Scanning surfaces are only considered complete when linked child tasks and verification evidence are complete
<!-- AC:END -->
