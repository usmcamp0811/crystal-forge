---
id: TASK-280
title: Make system detail rollback button open generation picker modal
status: Backlog
assignee: []
created_date: '2026-04-20 14:08'
labels:
  - systems
  - rollback
  - system-detail
  - ui
milestone: UI/UX parity
dependencies: []
references:
  - packages/web-ui/src/views/system_detail.rs
priority: medium
ordinal: 2780
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Problem: The rollback button in System Detail does not provide the expected rollback flow to pick a previous deployed generation.

Desired outcome: Clicking Rollback opens a modal that lists prior generations for the selected system and allows the user to select a target generation, confirm, and submit the rollback request.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Clicking Rollback in System Detail opens a modal instead of silently failing or doing nothing.
- [ ] #2 Modal lists previous generations available for rollback for that system.
- [ ] #3 User can select one generation and confirm rollback.
- [ ] #4 Rollback request uses selected generation/commit target and shows success/error feedback.
- [ ] #5 UI prevents invalid submission when no generation is selected.
<!-- AC:END -->
