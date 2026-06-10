---
id: TASK-339
title: Environments sidebar surface umbrella
status: Backlog
assignee: []
created_date: '2026-06-10 02:58'
updated_date: '2026-06-10 03:36'
labels:
  - design-parity
  - environments
  - web-ui
  - api-integration
milestone: 'm-20: Design Parity Missing Surfaces'
dependencies: []
references:
  - >-
    /home/mcamp/code/crystal-forge/CrystalForgelatest/components/EnvironmentsView.jsx
  - design/doc-13 - Sidebar-surface-execution-map.md
modified_files:
  - packages/web-ui/src/views/environments.rs
  - checks/web-ui/tests/integration-test.js
priority: high
ordinal: 1710
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Problem: Environments functionality exists, but the backlog needs one easy-to-read surface record for bringing the Environments experience into full parity with the CrystalForgelatest reference.

Desired Outcome: this task acts as the umbrella/surface record for Environments, with detailed discrepancy tasks added beneath it over time as needed.

Scope: may include direct implementation if the work remains small, but should primarily serve as the user-facing Environments planning anchor.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Environments view layout and controls match CrystalForgelatest
- [ ] #2 Primary Environments content and actions are backend-driven in production path
- [ ] #3 web-ui checks include screenshot and assertion coverage for Environments states
<!-- AC:END -->
