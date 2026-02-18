---
id: TASK-55
title: 'Refactor: Extract components from views/environments_list.rs'
status: To Do
assignee: []
created_date: '2026-02-18 02:47'
labels:
  - refactoring
  - web-ui
  - environments
dependencies: []
priority: low
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem
The views/environments_list.rs file is 891 lines and may contain components that should be extracted.

## Potential Components to Extract
Needs analysis, but may include:
- Environment cards
- Environment forms
- Environment-specific filters or tables

## Acceptance Criteria
- [ ] Analyze environments_list.rs for extractable components
- [ ] Create component files if warranted (components/environments/)
- [ ] Update views/environments_list.rs to import from components
- [ ] Target reduction: < 400 lines (if components extracted)
- [ ] Build passes: nix build .#checks.x86_64-linux.web-ui
<!-- SECTION:DESCRIPTION:END -->
