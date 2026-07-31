---
id: TASK-321.6
title: 'Dashboard: add Cache Health widget matching JSX design'
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
ordinal: 294000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem
The JSX design reference (`WCacheHealth`) includes a "Cache Health" widget (per-cache storage usage bars) not present in the Rust/Dioxus dashboard delivered in TASK-321.

## Desired Outcome
Add a `cache-health` widget visually matching JSX `WCacheHealth`:
- Up to 4 caches, each with mono name + percent and a usage progress bar (green/amber/red by status)
- Footer "N caches with issues" when applicable
- Header icon `download`, title "Cache Health", "View ->" navigates to caches

Back it with real cache destination data from the caches API.

## Notes
- Reference: WCacheHealth lines ~618-646 of DashboardView.jsx.
<!-- SECTION:DESCRIPTION:END -->
