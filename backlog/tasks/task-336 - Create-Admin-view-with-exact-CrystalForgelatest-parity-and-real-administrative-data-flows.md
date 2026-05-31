---
id: TASK-336
title: >-
  Create Admin view with exact CrystalForgelatest parity and real administrative
  data flows
status: To Do
assignee: []
created_date: '2026-05-31 16:02'
updated_date: '2026-05-31 16:07'
labels:
  - design-parity
  - admin
  - web-ui
  - api-integration
milestone: m-16
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

Scope details:
- Build/align Admin page structure and visual system to reference design.
- Implement design-specified controls/workflows (tables, filters, toggles, actions, dialogs) with exact interaction behavior.
- Ensure backend API integration for displayed values and action results.
- Standardize loading/empty/error/success states to design parity.

Verification plan:
- Add assertion-based Admin behavior checks in `checks/web-ui`.
- Capture Admin screenshots for all required states.
- Execute targeted web-ui parity validation.

Impact areas: packages/web-ui/src/views/admin.rs, admin components, backend handlers/models if required.
Risk: High.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Admin view visual layout is pixel-aligned with CrystalForgelatest references
- [ ] #2 Admin workflows/controls behave exactly as represented in design examples
- [ ] #3 Admin view data and outcomes are backend-driven with no production placeholder paths
- [ ] #4 web-ui check includes assertion-based validation for critical Admin workflows
- [ ] #5 web-ui check captures screenshots for Admin loading, empty, error, populated, and dialog states
<!-- AC:END -->
