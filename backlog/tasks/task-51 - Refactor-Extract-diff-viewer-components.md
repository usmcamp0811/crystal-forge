---
id: TASK-51
title: 'Refactor: Extract diff viewer components'
status: Backlog
assignee: ["KimiK2.5"]
created_date: '2026-02-18 02:46'
updated_date: '2026-02-19 03:39'
labels:
  - refactoring
  - web-ui
  - diff
dependencies: []
priority: low
milestone: m-3
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem
The components/diff/ directory exists but is empty (just TODO comments). Diff viewer components should be extracted from views.

## Components to Extract
Based on TODO in components/diff/mod.rs:
- DiffViewer (from system_detail.rs)
- FriendlyDiffViewer (from flakes_list.rs)

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Review views/system_detail.rs and views/flakes_list.rs for diff components
- [ ] #2 Create component files in components/diff/
- [ ] #3 Update components/diff/mod.rs with proper exports
- [ ] #4 Update views to import from components
- [ ] #5 Remove TODO comments from mod.rs
- [ ] #6 Build passes: nix build .#checks.x86_64-linux.web-ui
<!-- SECTION:DESCRIPTION:END -->
<!-- AC:END -->
