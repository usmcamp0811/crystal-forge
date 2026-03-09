---
id: TASK-123.2
title: Fix paginated list total field to return actual total count
status: To Do
assignee: []
created_date: '2026-03-09 20:59'
labels:
  - backend
  - api
  - bug
  - pagination
milestone: m-13
dependencies: []
parent_task_id: TASK-123
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

The `total` field in the paginated response is wrong.

The handler sets `total = policies.len()` after applying LIMIT/OFFSET, so `total` is just the page size, not the true total matching rows. That will give incorrect pagination metadata to the frontend or any API consumer.

For example, if there are 150 policies total but you request limit=50, the response incorrectly shows `total: 50` instead of `total: 150`.

## Goal

Add a separate COUNT query to return the actual total number of matching rows, independent of pagination parameters.

## Non-Goals

- Optimizing the COUNT query (basic SELECT COUNT(*) is sufficient)
- Adding filter parameters (future enhancement)
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Add count_deployment_policies() query function that returns total count
- [ ] #2 Handler calls both count and list queries
- [ ] #3 Response total field reflects actual total rows, not page size
- [ ] #4 Test with 150 policies requesting limit=50 returns total=150
- [ ] #5 Backend tests verify correct total across multiple pages
- [ ] #6 cargo test passes for deployment_policies module
<!-- AC:END -->
