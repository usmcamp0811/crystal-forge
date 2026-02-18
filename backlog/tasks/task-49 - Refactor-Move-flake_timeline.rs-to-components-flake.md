---
id: TASK-49
title: 'Refactor: Move flake_timeline.rs to components/flake/'
status: To Do
assignee: []
created_date: '2026-02-18 02:45'
labels:
  - refactoring
  - web-ui
  - flake
dependencies: []
priority: medium
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
- [ ] Move components/flake_timeline.rs to components/flake/flake_timeline.rs
- [ ] Update components/flake/mod.rs to export FlakeTimelineWidget
- [ ] Remove components/flake_timeline.rs
- [ ] Update imports in views/dashboard.rs and any other files using it
- [ ] Build passes: nix build .#checks.x86_64-linux.web-ui
<!-- SECTION:DESCRIPTION:END -->
