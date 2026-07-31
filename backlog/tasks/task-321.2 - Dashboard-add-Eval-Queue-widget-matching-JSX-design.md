---
id: TASK-321.2
title: 'Dashboard: add Eval Queue widget matching JSX design'
status: Backlog
assignee: []
created_date: '2026-06-09 02:42'
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
ordinal: 290000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem
The JSX design reference (`WEvalQueue`) includes an "Eval Queue" dashboard widget that is not present in the Rust/Dioxus dashboard delivered in TASK-321.

## Desired Outcome
Add an `eval-queue` widget to the dashboard widget registry visually matching JSX `WEvalQueue`:
- Big "active" count (purple #a78bfa)
- 2-column `dash-w-mini` stats: Completed (green) and Failed (red when > 0)
- Header icon `eval`, title "Eval Queue", "View ->" navigates to evaluations

Back it with real evaluation queue data from the evaluations API rather than mock `EVAL_STATS`.

## Notes
- Requires an evaluations summary/stats source on the dashboard (may need a small API addition or reuse of existing evaluations endpoints).
- Reference: WEvalQueue lines ~293-309 of DashboardView.jsx.
<!-- SECTION:DESCRIPTION:END -->
