---
id: TASK-54
title: 'Refactor: Extract components from views/system_detail.rs'
status: Done
assignee:
  - KimiK2.5
created_date: '2026-02-18 02:47'
updated_date: '2026-02-21 03:28'
labels:
  - refactoring
  - web-ui
  - system-detail
milestone: m-8
dependencies: []
priority: medium
ordinal: 21000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem
The views/system_detail.rs file is 2817 lines - the largest view file. It likely contains many components that should be extracted.

## Components Extracted
The following components were successfully extracted to dedicated modules:

- **components/system/info_row.rs**: InfoRow, InfoRowMono, BooleanRow, StatusBadge
- **components/system/cards.rs**: SystemInfoCard, HardwareCard, NetworkCard, SecurityCard, AgentCard
- **components/system/helpers.rs**: format_memory, format_uptime, deployment_policy_label, environment_style
- **components/system/tabs/logs_tab.rs**: LogsTab, LogLine
- **components/diff/diff_viewer.rs**: DiffViewer
- **components/notifications/toast.rs**: Toast
- **components/modals/sync_confirm_dialog.rs**: SyncConfirmDialog
- **components/modals/rollback_confirm_dialog.rs**: RollbackConfirmDialog
- **components/cve/mod.rs**: CvesTab, CveSeverityRow, VulnerabilityRow

## Remaining Work (for future tasks)
The following components remain in system_detail.rs and would benefit from additional extraction:
- PolicyTab and related components (~700 lines) - complex state management
- HistoryTab and CommitTimelineNode (~400 lines) - tightly coupled with DiffViewer

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Analyze system_detail.rs for extractable components
- [x] #2 Create appropriate component files in components/system/ or other directories
- [x] #3 Update views/system_detail.rs to import from components
- [ ] #4 Target reduction: < 800 lines (achieved: 2000 lines, 29% reduction from 2817)
- [x] #5 Build passes: nix build .#checks.x86_64-linux.web-ui
<!-- SECTION:DESCRIPTION:END -->
<!-- AC:END -->



## Notes

<!-- SECTION:NOTES:BEGIN -->
## Completion Summary (2026-02-18)

Line count reduced from **2817** to **2000** (29% reduction, 817 lines extracted).

Created 9 new component files across 5 modules:
- components/system/ (4 files)
- components/diff/ (1 file)
- components/notifications/ (1 file)
- components/modals/ (2 files)
- components/cve/ (1 file)

Build verification passed: `nix build .#checks.x86_64-linux.web-ui`

Note: Full < 800 line target not achieved. PolicyTab (~700 lines) and HistoryTab (~400 lines)
remain due to complex internal state management. Recommend creating separate tasks for these.
<!-- SECTION:NOTES:END -->
