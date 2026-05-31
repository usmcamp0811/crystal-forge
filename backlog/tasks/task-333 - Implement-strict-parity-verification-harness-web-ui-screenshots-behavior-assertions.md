---
id: TASK-333
title: >-
  Implement strict parity verification harness (web-ui screenshots + behavior
  assertions)
status: To Do
assignee: []
created_date: '2026-05-31 15:57'
updated_date: '2026-05-31 16:07'
labels:
  - design-parity
  - verification
  - web-ui-check
milestone: m-16
dependencies:
  - TASK-328
  - TASK-329
  - TASK-330
  - TASK-331
  - TASK-332
modified_files:
  - checks/web-ui
priority: high
ordinal: 1650
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Problem: Without objective automated verification, parity will regress and pixel-accurate claims are not defensible.

Goal: Extend checks/web-ui to enforce design parity with required screenshots and targeted behavior assertions for each in-scope view.

Non-goals: Broad E2E coverage unrelated to parity criteria.

Scope details:
- Capture canonical screenshots for every target view/state defined in parity matrix.
- Add assertions for key interactions (filtering, toggles, modal open/close, table/card mode switches, counts from API).
- Wire checks to fail on screenshot or assertion drift with clear diagnostics.

Verification plan:
- Run check locally and in CI path used by repository.
- Validate expected failure on intentional style drift.

Impact areas: checks/web-ui, Playwright/E2E fixtures (if used), test data setup.
Risk: Medium.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 web-ui check captures all required parity screenshots from the parity matrix
- [ ] #2 web-ui check asserts core design-required interactions for each primary view
- [ ] #3 Check output clearly identifies visual and behavior regressions
- [ ] #4 Parity tasks reference this harness as the acceptance proof mechanism
- [ ] #5 Screenshot coverage includes every in-scope view and state (loading, empty, error, populated, modal/tab variants)
- [ ] #6 Assertion coverage verifies critical user flows and state transitions for each in-scope view
<!-- AC:END -->
