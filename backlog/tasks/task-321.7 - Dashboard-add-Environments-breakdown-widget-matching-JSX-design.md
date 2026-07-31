---
id: TASK-321.7
title: 'Dashboard: add Environments breakdown widget matching JSX design'
status: Backlog
assignee: []
created_date: '2026-06-09 02:43'
labels:
  - ui
  - dashboard
  - parity
  - dioxus
milestone: 'm-6: UI Views - Dashboard'
dependencies: []
references:
  - >-
    /home/mcamp/code/crystal-forge/CrystalForgelatest/components/DashboardView.jsx
priority: medium
ordinal: 295000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem
The JSX design reference (`WEnvBreakdown`) includes an "Environments" widget (system count per environment with bars) not present in the Rust/Dioxus dashboard delivered in TASK-321.

## Desired Outcome
Add an `env-breakdown` widget visually matching JSX `WEnvBreakdown`:
- Per-environment rows sorted by count desc: color dot + mono name + count, with a proportional progress bar
- Header icon `env`, title "Environments", "View ->" navigates to environments

Back it with real environment + systems data (environments API + system counts).

## Notes
- Reference: WEnvBreakdown lines ~648-672 of DashboardView.jsx.
<!-- SECTION:DESCRIPTION:END -->
