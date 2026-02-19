---
id: TASK-32
title: Flakes view add/remove management
status: In Progress
assignee: ["KimiK2.5"]
created_date: '2026-02-17 02:20'
updated_date: '2026-02-19 04:06'
labels:
  - ui
  - web-ui
  - api
dependencies: []
priority: high
milestone: m-10
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Implement add and remove controls in Flakes view with backend API support based on flakes schema (name, repo_url unique).
<!-- SECTION:DESCRIPTION:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Refinement (2026-02-19 sprint review): keep In Progress. Frontend add/remove flow exists in flakes_list view, but task requires backend API support confirmation and end-to-end behavior validation against real handlers/routes.

Suggested remaining steps: document/confirm exact backend endpoints used for create/remove, add handler/query tests for add/remove semantics, and verify UI+API integration in dev stack.
<!-- SECTION:NOTES:END -->
