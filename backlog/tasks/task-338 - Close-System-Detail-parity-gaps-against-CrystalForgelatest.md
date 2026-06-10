---
id: TASK-338
title: Close System Detail parity gaps against CrystalForgelatest
status: Backlog
assignee: []
created_date: '2026-06-10 02:58'
labels:
  - design-parity
  - system-detail
  - web-ui
milestone: m-19
dependencies: []
references:
  - >-
    /home/mcamp/code/crystal-forge/CrystalForgelatest/components/SystemDetail.jsx
modified_files:
  - packages/web-ui/src/views/system_detail.rs
  - packages/web-ui/src/components/system/**
  - checks/web-ui/tests/integration-test.js
priority: high
ordinal: 1690
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Problem: System Detail behavior and visuals are currently split across several point tasks, but there is no single parity task ensuring the whole System Detail surface matches `/home/mcamp/code/crystal-forge/CrystalForgelatest/components/SystemDetail.jsx` end-to-end.

Desired Outcome: A single parity task coordinates the remaining System Detail work so overview, logs/history, hardening, tabs, spacing, modal/side-surface behavior, and screenshot/assertion coverage reach the same design standard as the reference.

Suggested execution: reuse and update existing sub-work such as TASK-268, TASK-277, TASK-295, and completed hardening/log tasks rather than rebuilding from scratch.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 System Detail layout and tab structure are pixel-aligned with CrystalForgelatest for desktop and mobile
- [ ] #2 Remaining System Detail interactions match design behavior, including history/log/detail affordances
- [ ] #3 All primary System Detail content is rendered from authoritative backend data with no placeholder paths
- [ ] #4 web-ui checks include screenshot and assertion coverage for the full System Detail surface
<!-- AC:END -->
