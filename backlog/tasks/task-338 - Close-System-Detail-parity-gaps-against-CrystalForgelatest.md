---
id: TASK-338
title: System Detail sidebar surface umbrella
status: Backlog
assignee: []
created_date: '2026-06-10 02:58'
updated_date: '2026-06-10 03:36'
labels:
  - design-parity
  - system-detail
  - web-ui
milestone: 'm-19: Design Parity Existing Surfaces'
dependencies: []
references:
  - >-
    /home/mcamp/code/crystal-forge/CrystalForgelatest/components/SystemDetail.jsx
  - TASK-268
  - TASK-277
  - TASK-295
  - design/doc-13 - Sidebar-surface-execution-map.md
modified_files:
  - packages/web-ui/src/views/system_detail.rs
  - packages/web-ui/src/components/system/**
  - checks/web-ui/tests/integration-test.js
priority: high
ordinal: 1690
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Problem: System Detail behavior and visuals are currently split across several point tasks, but there is no single easy-to-read surface record ensuring the whole System Detail experience matches `/home/mcamp/code/crystal-forge/CrystalForgelatest/components/SystemDetail.jsx` end-to-end.

Desired Outcome: this task serves as the umbrella record for the System Detail surface so overview, logs/history, hardening, tabs, spacing, modal behavior, and screenshot/assertion coverage can be tracked as one user-facing thing.

Scope: planning/coordination only. Direct implementation belongs in detailed discrepancy tasks such as history/logs/icons/hardening follow-ups.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 System Detail layout and tab structure are pixel-aligned with CrystalForgelatest for desktop and mobile
- [ ] #2 Remaining System Detail interactions match design behavior, including history/log/detail affordances
- [ ] #3 All primary System Detail content is rendered from authoritative backend data with no placeholder paths
- [ ] #4 web-ui checks include screenshot and assertion coverage for the full System Detail surface
<!-- AC:END -->
