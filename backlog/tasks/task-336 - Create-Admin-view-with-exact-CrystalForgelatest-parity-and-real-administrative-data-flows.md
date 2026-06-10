---
id: TASK-336
title: >-
  Create Admin view with exact CrystalForgelatest parity and real administrative
  data flows
status: Backlog
assignee: []
created_date: '2026-05-31 16:02'
updated_date: '2026-06-10 02:57'
labels:
  - design-parity
  - admin
  - web-ui
  - api-integration
milestone: m-20
dependencies:
  - TASK-328
  - TASK-329
  - TASK-332
  - TASK-333
references:
  - /home/mcamp/code/crystal-forge/CrystalForgelatest
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

Goal: Implement/refine Admin view to match CrystalForgelatest exactly, including interaction patterns and authoritative data rendering.

Non-goals: New admin capabilities not represented in design scope; unrelated IAM redesign.

Replan note: this is an m-20 missing-surface task and should be executed as a vertical slice with the minimal real-data backend support required.

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
Prefer vertical-slice completion with real admin data and outcomes, not placeholder-first UI.
<!-- SECTION:NOTES:END -->
