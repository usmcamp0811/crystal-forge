---
id: TASK-334
title: >-
  Create Compliance view with exact CrystalForgelatest parity and backend-backed
  data
status: To Do
assignee: []
created_date: '2026-05-31 16:02'
updated_date: '2026-05-31 16:07'
labels:
  - design-parity
  - compliance
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

Scope details:
- Create/complete Compliance view surface with exact visual parity (spacing, typography, tokens, cards/tables/chips, controls).
- Implement required interactions from design (filters/search/tabs/actions/modals as applicable).
- Ensure all primary displayed values are backend-driven (no production mock/fallback placeholders).
- Align empty/loading/error/populated states with design source.

Verification plan:
- Extend `checks/web-ui` with assertion-based tests for Compliance interactions and state transitions.
- Capture screenshot evidence for all Compliance states.
- Run targeted web-ui parity check.

Impact areas: packages/web-ui/src/views/compliance*.rs (or equivalent), supporting components, API model adapters if required.
Risk: High.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Compliance view visual layout is pixel-aligned with CrystalForgelatest references for desktop and mobile
- [ ] #2 Compliance interactions match design behavior for all in-scope controls
- [ ] #3 Primary Compliance content is rendered from backend APIs in production path
- [ ] #4 web-ui check includes assertion-based validation of Compliance interactions and state transitions
- [ ] #5 web-ui check captures screenshots for Compliance loading, empty, error, populated, and modal/tab states
<!-- AC:END -->
