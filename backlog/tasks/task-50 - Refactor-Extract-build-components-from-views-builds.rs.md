---
id: TASK-50
title: 'Refactor: Extract build components from views/builds.rs'
status: Done
assignee: []
created_date: '2026-02-18 02:45'
updated_date: '2026-02-19 13:16'
labels:
  - refactoring
  - web-ui
  - builds
dependencies: []
priority: low
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem
The components/builds/ directory exists but is empty (just TODO comments). Components should be extracted from views/builds.rs.

## Components to Extract (from builds.rs)
Based on TODO in components/builds/mod.rs:
- MetricsRow
- MetricBadge
- WorkerStrip
- BuildQueuePane
- BuildDetailPane

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Review views/builds.rs for extractable components
- [ ] #2 Create component files in components/builds/
- [ ] #3 Update components/builds/mod.rs with proper exports
- [ ] #4 Update views/builds.rs to import from components
- [ ] #5 Remove TODO comments from mod.rs
- [ ] #6 Build passes: nix build .#checks.x86_64-linux.web-ui
<!-- SECTION:DESCRIPTION:END -->

<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
LOCK: OpenCode on gray in /home/mcamp/code/crystal-forge/TASK-50-extract-build-components

Extracted 5 components from views/builds.rs into components/builds/: MetricsRow, MetricBadge, WorkerStrip, BuildQueuePane, BuildDetailPane. Created helpers.rs for shared types and styling functions. Updated mod.rs with proper exports. View now imports from components.

Verification: nix build .#checks.x86_64-linux.web-ui passed.

Closed from MR audit: merged in MR !113 (TASK-50-extract-build-components -> dev).
<!-- SECTION:NOTES:END -->
