---
id: TASK-123.1
title: Fix list_deployment_policies to return all policies for CRUD management
status: Done
assignee: []
created_date: '2026-03-09 20:59'
updated_date: '2026-03-13 01:24'
labels:
  - backend
  - api
  - bug
  - policies
milestone: m-13
dependencies: []
parent_task_id: TASK-123
priority: high
ordinal: 4000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

The list endpoint only returns enabled policies, which breaks the CRUD UX for disabled policies.

`list_deployment_policies()` queries `deployment_policies WHERE enabled = true`, and the UI loads its main list from that endpoint. That means once a policy is disabled, it disappears from the management page and can no longer be edited or re-enabled through the UI.

The evaluator should load only enabled policies (use `list_enabled_deployment_policies()`), but the admin CRUD list should show ALL policies by default.

## Goal

Remove the `WHERE enabled = true` filter from the main `list_deployment_policies()` query used by the CRUD API endpoint, so administrators can see and manage both enabled and disabled policies.

## Non-Goals

- Adding filter parameters to the endpoint (can be a future enhancement)
- Changing the evaluator's query (it should still use `list_enabled_deployment_policies()`)
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 list_deployment_policies() query removes WHERE enabled = true filter
- [x] #2 GET /api/v1/deployment-policies returns both enabled and disabled policies
- [x] #3 list_enabled_deployment_policies() query remains unchanged (still filters enabled only)
- [x] #4 Backend tests verify all policies are returned regardless of enabled status
- [x] #5 cargo test passes for deployment_policies module
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
LOCK: claude-agent on gray in ~/code/crystal-forge/TASK-123-deployment-policies-crud

Fixed: Removed WHERE enabled = true filter from list_deployment_policies query

Added test to verify both enabled and disabled policies are returned

Changes committed: 8976cbe8
<!-- SECTION:NOTES:END -->
