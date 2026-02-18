---
id: TASK-54
title: 'Refactor: Extract components from views/system_detail.rs'
status: To Do
assignee: []
created_date: '2026-02-18 02:47'
labels:
  - refactoring
  - web-ui
  - system-detail
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem
The views/system_detail.rs file is 2817 lines - the largest view file. It likely contains many components that should be extracted.

## Potential Components to Extract
Needs analysis, but likely includes:
- System info cards/panels
- Diff viewer (mentioned in diff/ TODO)
- Hardware info display
- Network info display
- Security info display
- Action buttons/controls

## Acceptance Criteria
- [ ] Analyze system_detail.rs for extractable components
- [ ] Create appropriate component files in components/system/ or other directories
- [ ] Update views/system_detail.rs to import from components
- [ ] Target reduction: < 800 lines
- [ ] Build passes: nix build .#checks.x86_64-linux.web-ui
<!-- SECTION:DESCRIPTION:END -->
