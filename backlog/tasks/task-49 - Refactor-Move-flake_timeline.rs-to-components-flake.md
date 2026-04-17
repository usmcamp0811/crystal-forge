---
id: TASK-49
title: 'Refactor: Move flake_timeline.rs to components/flake/'
status: Done
assignee:
  - KimiK2.5
created_date: '2026-02-18 02:45'
updated_date: '2026-03-13 01:24'
labels:
  - refactoring
  - web-ui
  - flake
milestone: m-10
dependencies: []
priority: medium
ordinal: 25000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem
The flake_timeline.rs component is at the top level of components/ but should be in the domain-specific components/flake/ directory.

## Current Location
```
components/
├── flake_timeline.rs    # Should be in flake/
├── flake/
│   └── mod.rs           # Currently just has TODO comments
```

## Target Location
```
components/
├── flake/
│   ├── mod.rs           # Should export FlakeTimelineWidget
│   └── flake_timeline.rs
```

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Move components/flake_timeline.rs to components/flake/flake_timeline.rs
- [ ] #2 Update components/flake/mod.rs to export FlakeTimelineWidget
- [ ] #3 Remove components/flake_timeline.rs
- [ ] #4 Update imports in views/dashboard.rs and any other files using it
- [ ] #5 Build passes: nix build .#checks.x86_64-linux.web-ui
<!-- SECTION:DESCRIPTION:END -->

<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Moved components/flake_timeline.rs to components/flake/flake_timeline.rs and updated imports.

## Already Complete
The flake_timeline component was already correctly positioned:
- components/flake/flake_timeline.rs exists (36793 bytes)
- components/flake/mod.rs properly exports FlakeTimelineWidget
- All imports in views/dashboard.rs use crate::components::flake::FlakeTimelineWidget

## Verified
- Build passes: cargo check succeeds
<!-- SECTION:NOTES:END -->
