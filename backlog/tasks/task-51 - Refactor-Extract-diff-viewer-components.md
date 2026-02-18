---
id: TASK-51
title: 'Refactor: Extract diff viewer components'
status: To Do
assignee: []
created_date: '2026-02-18 02:46'
labels:
  - refactoring
  - web-ui
  - diff
dependencies: []
priority: low
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
- [ ] Review views/system_detail.rs and views/flakes_list.rs for diff components
- [ ] Create component files in components/diff/
- [ ] Update components/diff/mod.rs with proper exports
- [ ] Update views to import from components
- [ ] Remove TODO comments from mod.rs
- [ ] Build passes: nix build .#checks.x86_64-linux.web-ui
<!-- SECTION:DESCRIPTION:END -->
