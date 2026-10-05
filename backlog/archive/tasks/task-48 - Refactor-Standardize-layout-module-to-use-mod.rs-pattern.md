---
id: TASK-48
title: 'Refactor: Standardize layout module to use mod.rs pattern'
status: Done
assignee:
  - KimiK2.5
created_date: '2026-02-18 02:45'
updated_date: '2026-05-26 03:34'
labels:
  - refactoring
  - web-ui
  - layout
milestone: m-3
dependencies: []
priority: medium
ordinal: 55000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem
The components/layout module uses an inconsistent pattern compared to other component modules.

## Current Structure (Inconsistent)
```mermaid
%% diagram-id: backlog-task48-current-layout-module-tree
flowchart TD
    components["components/"] --> layout_rs["layout.rs — Module declaration file (alternate pattern)"]
    components --> layout_dir["layout/"]
    layout_dir --> app_shell["app_shell.rs"]
    layout_dir --> card["card.rs"]
    layout_dir --> sidebar["sidebar.rs"]
    layout_dir --> topbar["topbar.rs"]
```

## Target Structure (Standard)
```mermaid
%% diagram-id: backlog-task48-target-layout-module-tree
flowchart TD
    components["components/"] --> layout["layout/"]
    layout --> mod_rs["mod.rs — Module declaration (standard pattern)"]
    layout --> app_shell["app_shell.rs"]
    layout --> card["card.rs"]
    layout --> sidebar["sidebar.rs"]
    layout --> topbar["topbar.rs"]
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

## Already Complete
The layout module was already correctly structured:
- components/layout/mod.rs exists and properly exports all components
- No components/layout.rs file exists (old pattern removed)

## Verified
- All imports continue to work
- Build passes
<!-- SECTION:NOTES:END -->
