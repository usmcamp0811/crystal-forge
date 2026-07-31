---
id: TASK-353.1
title: Add persistent system tags backend and API support
status: Backlog
assignee: []
created_date: '2026-06-14 01:39'
labels:
  - systems
  - backend
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
System Detail and Edit System modal now render design-parity editable tags as local UI state only. The backend has no `systems.tags` column or system_tags table and `UpdateSystemRequest` cannot persist tags.

## Desired Outcome
Add persistent system tags support so users can add/remove tags on systems and have those tags survive reloads and be available to list/detail filtering.

## Notes
Created as follow-up from TASK-353. Because the dev server has live migrations applied, implement any schema change as a NEW migration file only; do not edit existing migrations.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 System tags are persisted in the database via a new migration file
- [ ] #2 System detail API includes tags
- [ ] #3 System update API can add/remove or replace tags
- [ ] #4 Web UI System Detail and Edit modal use persisted tags instead of local-only state
- [ ] #5 Tests cover tag persistence and API serialization
<!-- AC:END -->
