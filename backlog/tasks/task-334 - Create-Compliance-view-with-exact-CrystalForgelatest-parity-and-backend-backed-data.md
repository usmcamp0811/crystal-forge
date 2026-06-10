---
id: TASK-334
title: >-
  Create Compliance view with exact CrystalForgelatest parity and backend-backed
  data
status: Backlog
assignee: []
created_date: '2026-05-31 16:02'
updated_date: '2026-06-10 02:57'
labels:
  - design-parity
  - compliance
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
  - packages/web-ui/src/views/compliance.rs
  - checks/web-ui
priority: high
ordinal: 1660
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Problem: Compliance view is not fully implemented to match latest design standards and may rely on incomplete or placeholder states.

Goal: Implement the Compliance view so it matches CrystalForgelatest layout/interactions pixel-for-pixel and renders authoritative backend data for all primary states.

Non-goals: Broad redesign of unrelated views; speculative compliance features outside reference design.

Replan note: this is a net-new/missing-surface task in m-20. Prefer vertical-slice delivery: required UI plus the minimal real-data backend support needed for the view to be truthful.

Scope details:
- Create/complete Compliance view surface with exact visual parity.
- Implement required interactions from design (filters/search/tabs/actions/modals as applicable).
- Ensure all primary displayed values are backend-driven.
- Align empty/loading/error/populated states with design source.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Compliance view visual layout is pixel-aligned with CrystalForgelatest references for desktop and mobile
- [ ] #2 Compliance interactions match design behavior for all in-scope controls
- [ ] #3 Primary Compliance content is rendered from backend APIs in production path
- [ ] #4 web-ui check includes assertion-based validation of Compliance interactions and state transitions
- [ ] #5 web-ui check captures screenshots for Compliance loading, empty, error, populated, and modal/tab states
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Execute as a vertical slice: UI plus required authoritative data, not placeholder-first UI.
<!-- SECTION:NOTES:END -->
