---
id: TASK-23
title: Dashboard Flake Commit Timeline Widget
status: Done
assignee: []
created_date: '2026-02-14 05:43'
updated_date: '2026-02-14 05:50'
labels:
  - ui
  - dashboard
  - components
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Add a horizontal timeline widget to the dashboard showing git commits for monitored flakes. Each commit shows the count of systems deployed at that version. Color-coded severity gradient highlights systems that are behind (yellow = 1 commit behind, orange = 2, red = 3+).

Features:
- Horizontal timeline flowing left-to-right (oldest to newest on right)
- Each commit node shows: short hash, commit message preview, system count badge
- System count badge color indicates how current that commit is vs latest
- Gradient severity: green (latest), yellow (1 behind), orange (2 behind), red (3+ behind)
- Toggle between: single combined timeline (all flakes) OR stacked per-flake timelines OR filtered to one flake
- Clicking a commit could expand to show which systems are on that version

Mock data for development:
- 2-3 flakes with 5-10 commits each
- Systems distributed across commits (some on latest, some behind)

Architecture:
- New FlakeTimeline component in packages/web-ui/src/components/
- Add to dashboard below recent deployments
- DTOs for FlakeCommit, CommitTimelineEntry in api/models.rs
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Horizontal timeline renders with commits left-to-right (oldest to newest)
- [x] #2 Each commit shows short hash, message preview, and system count
- [x] #3 Severity gradient coloring: green (latest), yellow (1 behind), orange (2 behind), red (3+)
- [x] #4 View toggle: combined timeline / stacked per-flake / filter to single flake
- [x] #5 Mock data with 2-3 flakes and realistic commit history
- [x] #6 Playwright test assertions verify timeline renders correctly
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented flake commit timeline widget:
- Horizontal timeline with commits flowing left-to-right (oldest to newest)
- Each commit shows short hash, message preview, and system count bar
- Bar height scales with system count (min 20px, max 80px)
- Severity gradient: green (latest), yellow (1 behind), orange (2 behind), red (3+)
- View toggle: Combined (all flakes merged), Stacked (per-flake), Filter (single flake dropdown)
- Legend explains color coding
- Mock data with 3 flakes (infrastructure, workstations, edge-nodes) and realistic commits
- Playwright assertions verify timeline renders correctly
<!-- SECTION:NOTES:END -->
