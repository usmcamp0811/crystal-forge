---
id: TASK-336
title: Admin sidebar surface umbrella
status: Backlog
assignee: []
created_date: '2026-05-31 16:02'
updated_date: '2026-06-21 02:08'
labels:
  - design-parity
  - admin
  - web-ui
  - api-integration
milestone: 'm-20: Design Parity Missing Surfaces'
dependencies:
  - TASK-328
  - TASK-329
  - TASK-333
references:
  - /home/mcamp/code/crystal-forge/CrystalForgelatest
  - design/doc-13 - Sidebar-surface-execution-map.md
modified_files:
  - packages/web-ui/src/views/admin.rs
  - packages/server/src
  - checks/web-ui
priority: high
ordinal: 1670
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Problem: Admin view does not yet fully match latest design standards and may not provide complete backend-backed behavior for key administrative surfaces.

Goal: This task serves as the umbrella record for the Admin sidebar surface and tracks the work required to match CrystalForgelatest with authoritative data and correct interactions.

Non-goals: New admin capabilities not represented in design scope; unrelated IAM redesign.

Replan note: this is the primary Admin surface record. If implementation scope expands, add focused discrepancy tasks beneath it rather than overloading one branch/worktree.

Scope details:
- Build/align Admin page structure and visual system to reference design.
- Implement design-specified controls/workflows with exact interaction behavior.
- Ensure backend API integration for displayed values and action results.
- Standardize loading/empty/error/success states to design parity.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Admin view visual layout is pixel-aligned with CrystalForgelatest references
- [ ] #2 Admin workflows/controls behave exactly as represented in design examples
- [ ] #3 Admin view data and outcomes are backend-driven with no production placeholder paths
- [ ] #4 web-ui check includes assertion-based validation for critical Admin workflows
- [ ] #5 web-ui check captures screenshots for Admin loading, empty, error, populated, and dialog states
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Primary Admin surface record under the sidebar-oriented plan.
<!-- SECTION:NOTES:END -->
