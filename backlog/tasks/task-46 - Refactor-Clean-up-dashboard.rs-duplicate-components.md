---
id: TASK-46
title: 'Refactor: Clean up dashboard.rs duplicate components'
status: In Progress
assignee: []
created_date: '2026-02-18 02:44'
updated_date: '2026-02-18 02:55'
labels:
  - refactoring
  - web-ui
  - dashboard
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem
The views/dashboard.rs file is 1900+ lines and contains duplicate definitions of components that already exist in components/dashboard/.

## Current Duplicates
| Component | Defined In | Should Be |
|-----------|------------|-----------|
| BuildQueuePanel | views/dashboard.rs + components/dashboard/build_queue.rs | Import from components |
| BuildQueueRow | views/dashboard.rs + components/dashboard/build_queue.rs | Import from components |
| CveSummaryPanel | views/dashboard.rs + components/dashboard/cve_summary.rs | Import from components |
| FleetHealthBreakdown | views/dashboard.rs + components/dashboard/fleet_health.rs | Import from components |
| DeploymentStatusBreakdown | views/dashboard.rs + components/dashboard/deployment_status.rs | Import from components |
| RecentDeploymentsList | views/dashboard.rs + components/dashboard/recent_deployments.rs | Import from components |
| BuildSummaryPanel | views/dashboard.rs + components/dashboard/build_summary.rs | Import from components |
| DonutChartWithLegend | views/dashboard.rs | Already in components/charts/donut.rs |
| DonutSegment | views/dashboard.rs | Already in components/charts/donut.rs |
| DonutArc | views/dashboard.rs | Already in components/charts/donut.rs |
| format_time_ago | views/dashboard.rs | Already in components/dashboard/mod.rs |
| format_elapsed | views/dashboard.rs | Already in components/dashboard/mod.rs |

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 views/dashboard.rs reduced to ~300-400 lines
- [ ] #2 All components imported from components/dashboard/ and components/charts/
- [ ] #3 No duplicate component definitions
- [ ] #4 Mock data functions (mock_dashboard_summary, mock_flake_timelines, mock_build_queue_summary) can remain or move to api/mock_data.rs
- [ ] #5 Build passes: nix build .#checks.x86_64-linux.web-ui
<!-- SECTION:DESCRIPTION:END -->
<!-- AC:END -->
