---
id: TASK-48
title: 'Refactor: Standardize layout module to use mod.rs pattern'
status: Done
assignee: []
created_date: '2026-02-18 02:45'
updated_date: '2026-02-18 02:50'
labels:
  - refactoring
  - web-ui
  - layout
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem
The components/layout module uses an inconsistent pattern compared to other component modules.

## Current Structure (Inconsistent)
```
components/
├── layout.rs          # Module declaration file (alternate pattern)
├── layout/
│   ├── app_shell.rs
│   ├── card.rs
│   ├── sidebar.rs
│   └── topbar.rs
```

## Target Structure (Standard)
```
components/
├── layout/
│   ├── mod.rs         # Module declaration (standard pattern)
│   ├── app_shell.rs
│   ├── card.rs
│   ├── sidebar.rs
│   └── topbar.rs
```

## Other modules use the standard pattern:
- components/dashboard/mod.rs
- components/charts/mod.rs
- components/filters/mod.rs
- components/modals/mod.rs
- components/tables/mod.rs

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Delete components/layout.rs
- [ ] #2 Create components/layout/mod.rs with same content
- [ ] #3 All imports continue to work
- [ ] #4 Build passes: nix build .#checks.x86_64-linux.web-ui
<!-- SECTION:DESCRIPTION:END -->

<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Renamed components/layout.rs to components/layout/mod.rs to match standard module pattern used by other component modules.
<!-- SECTION:NOTES:END -->
