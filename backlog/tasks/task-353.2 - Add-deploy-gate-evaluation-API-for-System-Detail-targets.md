---
id: TASK-353.2
title: Add deploy-gate evaluation API for System Detail targets
status: Backlog
assignee: []
created_date: '2026-06-14 01:39'
labels:
  - systems
  - deployments
  - policy
  - api
  - web-ui
  - follow-up
milestone: 'm-19: Design Parity Existing Surfaces'
dependencies: []
references:
  - TASK-353
priority: medium
ordinal: 0
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem
The System Detail Deploy tab now renders a design-parity Deploy gate panel, but there is no HTTP endpoint that evaluates policy gates for a selected system + candidate commit/generation. Existing gate evaluation runs only inside the server-side deployment loop.

## Desired Outcome
Expose real deploy-gate evaluation data for System Detail so the UI can replace the derived placeholder gate summary with authoritative backend policy results.

## Notes
Created as follow-up from TASK-353. Because the dev server has live migrations applied, implement any schema changes as NEW migration files only; do not edit existing migrations.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Backend exposes a system target deploy-gate evaluation endpoint
- [ ] #2 Endpoint returns per-rule status, reasons, next actions, and overall status
- [ ] #3 System Detail Deploy tab consumes endpoint data for selected commit/generation
- [ ] #4 UI handles loading, pending, warning, blocked, and allowed gate states
- [ ] #5 Tests cover endpoint behavior and UI rendering
<!-- AC:END -->
