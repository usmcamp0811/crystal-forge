---
id: TASK-196
title: >-
  Dashboard pipeline readiness widget doesn't support scrolling for multiple
  errors/warnings
status: To Do
assignee: []
created_date: '2026-03-19 12:36'
updated_date: '2026-04-07 02:12'
labels:
  - bug
  - ui
  - dashboard
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem Statement
The dashboard Pipeline Readiness widget does not support scrolling for long lists of warnings/errors, so items overflow and become inaccessible.

## Goal
Add bounded vertical scrolling to the Pipeline Readiness issue list so all warnings/errors are visible without breaking card layout.

## Non-Goals
- No change to readiness scoring logic or backend data contracts.
- No policy rule changes.
- No dashboard-wide style redesign.

## Architectural Constraints
- Keep change in web-ui presentation layer only.
- Preserve current data model contracts and component boundaries.
- Match existing dashboard UX conventions for scrollable card content.

## Verification Plan
- Run targeted web-ui checks/build for changed component(s).
- Validate rendering with multiple readiness errors/warnings.
- Confirm no overflow/regression in adjacent dashboard widgets.

## Impact Areas
- Pipeline readiness dashboard component(s) under `packages/web-ui/src/components/dashboard/`
- Relevant CSS in `packages/web-ui/assets/app.css` (if needed)

## Risk Level
Low to Medium: UI overflow/scroll behavior can affect card height consistency.

## Dependencies
None.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Given multiple readiness errors/warnings beyond available card height, users can scroll to view all items.
- [ ] #2 Readiness items stay within widget boundaries without visual overflow.
- [ ] #3 Users receive a visible cue that content is scrollable (native scrollbar or equivalent).
- [ ] #4 Dashboard layout remains stable with no clipping/overlap regressions in neighboring cards.
- [ ] #5 Targeted web-ui verification passes for modified files.
<!-- AC:END -->
