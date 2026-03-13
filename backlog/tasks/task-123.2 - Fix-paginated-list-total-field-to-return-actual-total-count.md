---
id: TASK-123.2
title: Fix paginated list total field to return actual total count
status: Done
assignee: []
created_date: '2026-03-09 20:59'
updated_date: '2026-03-13 01:24'
labels:
  - backend
  - api
  - bug
  - pagination
milestone: m-13
dependencies: []
parent_task_id: TASK-123
priority: high
ordinal: 3500
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
- [x] #1 Add count_deployment_policies() query function that returns total count
- [x] #2 Handler calls both count and list queries
- [x] #3 Response total field reflects actual total rows, not page size
- [x] #4 Test with 150 policies requesting limit=50 returns total=150
- [x] #5 Backend tests verify correct total across multiple pages
- [x] #6 cargo test passes for deployment_policies module
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
LOCK: claude-agent on gray in ~/code/crystal-forge/TASK-123-deployment-policies-crud

Fixed: Added count_deployment_policies() query function

Handler now calls count query before fetching policies

Response total field now reflects actual total rows, not page size

Added tests to verify correct pagination totals

Changes committed: 8976cbe8
<!-- SECTION:NOTES:END -->
