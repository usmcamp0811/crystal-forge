---
id: TASK-281
title: Implement functional SSH action in system detail view
status: Backlog
assignee: []
created_date: '2026-04-20 14:09'
labels:
  - systems
  - system-detail
  - ssh
  - low-priority
milestone: UI/UX parity
dependencies: []
references:
  - packages/web-ui/src/views/system_detail.rs
priority: low
ordinal: 2810
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Problem: The SSH button on System Detail is currently non-functional.

Desired outcome: Provide a working SSH action flow (or explicit UX fallback) from the system detail page. This is low priority and should be planned separately from current UI refinements.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Clicking SSH in System Detail performs a defined action (e.g., copy command, open terminal helper, or deep-link flow) instead of doing nothing.
- [ ] #2 Behavior handles unavailable SSH connection details gracefully with clear user feedback.
- [ ] #3 Feature is permission-aware if role constraints apply.
- [ ] #4 No regressions to existing System Detail header actions.
<!-- AC:END -->
