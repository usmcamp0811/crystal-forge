---
id: TASK-321.4
title: 'Dashboard: add Recent Commits widget matching JSX design'
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
parent_task_id: TASK-321
priority: medium
ordinal: 292000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem
The JSX design reference (`WRecentCommits`) includes a "Recent Commits" list widget not present in the Rust/Dioxus dashboard delivered in TASK-321.

## Desired Outcome
Add a `recent-commits` widget visually matching JSX `WRecentCommits`:
- Height-resizable list of latest commits across tracked flakes
- Each row: mono sha (purple), message (ellipsis), flake name, relative time
- Header icon `git`, title "Recent Commits", "View ->" navigates to flakes

Back it with real flake commit data (already loaded as flake timelines on the dashboard).

## Notes
- Data is already available via flake timelines; this is mostly a presentation widget.
- Reference: WRecentCommits lines ~596-616 of DashboardView.jsx.
<!-- SECTION:DESCRIPTION:END -->
