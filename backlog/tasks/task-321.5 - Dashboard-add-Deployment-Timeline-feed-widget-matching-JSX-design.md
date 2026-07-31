---
id: TASK-321.5
title: 'Dashboard: add Deployment Timeline feed widget matching JSX design'
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
ordinal: 293000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem
The JSX design reference (`WDeploymentTimeline`) includes a chronological activity-feed widget (deploys/builds/evals/failures with a connector line) not present in the Rust/Dioxus dashboard delivered in TASK-321.

## Desired Outcome
Add a `deployment-timeline` widget visually matching JSX `WDeploymentTimeline`:
- Height-resizable vertical timeline with colored icon nodes + connector line
- Items: recent deploys, active builds, active evals, failed builds
- Each item: title (with mono refs), relative time, optional EnvBadge + sub text
- Header icon `history`, title "Deployment Timeline", "View ->" navigates to systems

Back it with real recent activity (recent_deployments + build/eval activity), not the JSX mock feed.

## Notes
- Distinct from the existing "Recent Deployments" widget which is a flat list.
- Reference: WDeploymentTimeline lines ~503-594 of DashboardView.jsx.
<!-- SECTION:DESCRIPTION:END -->
