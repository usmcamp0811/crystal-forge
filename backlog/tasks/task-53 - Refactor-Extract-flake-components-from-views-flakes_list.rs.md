---
id: TASK-53
title: 'Refactor: Extract flake components from views/flakes_list.rs'
status: To Do
assignee: []
created_date: '2026-02-18 02:46'
labels:
  - refactoring
  - web-ui
  - flake
dependencies: []
priority: low
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
- [ ] Review views/flakes_list.rs for extractable components
- [ ] Create component files in components/flake/
- [ ] Update components/flake/mod.rs with proper exports
- [ ] Update views/flakes_list.rs to import from components
- [ ] Remove TODO comments from mod.rs
- [ ] Build passes: nix build .#checks.x86_64-linux.web-ui
<!-- SECTION:DESCRIPTION:END -->
