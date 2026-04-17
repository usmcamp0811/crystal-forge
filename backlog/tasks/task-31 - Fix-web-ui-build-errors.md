---
id: TASK-31
title: Fix web-ui build errors
status: Done
assignee:
  - KimiK2.5
created_date: '2026-02-16 17:35'
updated_date: '2026-03-13 01:24'
labels:
  - ui
  - build
milestone: m-3
dependencies: []
priority: high
ordinal: 31000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
nix build .#checks.x86_64-linux.web-ui failed with existing errors in packages/web-ui/src/views/system_detail.rs (on_format_change mutability) and warnings in packages/web-ui/src/views/systems_list.rs (unused mut).
<!-- SECTION:DESCRIPTION:END -->

## Notes

<!-- SECTION:NOTES:BEGIN -->
## Completion Summary (2026-02-18)

Fixed compiler warnings by removing unnecessary `mut` from `use_signal` declarations:

### Files Modified:
- **packages/web-ui/src/views/builds.rs**: Removed `mut` from 4 signals (follow_logs, pause_logs, wrap_logs, log_query)
- **packages/web-ui/src/views/flakes_list.rs**: Removed `mut` from 5 signals (open_dropdown, selected_history_flake, selected_history_commit, sort_column, sort_direction), fixed import ordering
- **packages/web-ui/src/views/system_detail.rs**: Removed `mut` from policy_library signal in PolicyTab
- **packages/web-ui/src/views/systems_list.rs**: Removed `mut` from open_dropdown signal

### Verification:
Build passes: `nix build .#checks.x86_64-linux.web-ui`
<!-- SECTION:NOTES:END -->
