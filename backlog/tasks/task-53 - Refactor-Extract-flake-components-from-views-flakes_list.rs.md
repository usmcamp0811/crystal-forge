---
id: TASK-53
title: 'Refactor: Extract flake components from views/flakes_list.rs'
status: Backlog
assignee: []
created_date: '2026-02-18 02:46'
updated_date: '2026-02-19 03:39'
labels:
  - refactoring
  - web-ui
  - flake
dependencies: []
priority: low
milestone: m-10
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem
The components/flake/ directory has TODO comments indicating components should be extracted from views/flakes_list.rs.

## Components to Extract
Based on TODO in components/flake/mod.rs:
- FlakeCard
- FlakeHistoryExplorer
- FriendlyDiffViewer (also mentioned in diff/ TODO)

## Note
FlakeTimelineWidget already identified for move in TASK-49

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Review views/flakes_list.rs for extractable components
- [ ] #2 Create component files in components/flake/
- [ ] #3 Update components/flake/mod.rs with proper exports
- [ ] #4 Update views/flakes_list.rs to import from components
- [ ] #5 Remove TODO comments from mod.rs
- [ ] #6 Build passes: nix build .#checks.x86_64-linux.web-ui
<!-- SECTION:DESCRIPTION:END -->
<!-- AC:END -->
