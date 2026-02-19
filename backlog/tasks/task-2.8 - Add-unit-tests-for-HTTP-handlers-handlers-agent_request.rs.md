---
id: TASK-2.8
title: Add unit tests for HTTP handlers - handlers/agent_request.rs
status: Backlog
assignee: []
created_date: '2026-02-04 20:39'
updated_date: '2026-02-19 03:39'
labels:
  - testing
  - handlers
  - http
milestone: m-1
dependencies: []
parent_task_id: TASK-2
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Test authentication logic in isolation using mock requests and keys.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Test authenticate_agent_request with valid signature
- [ ] #2 Test with invalid signature
- [ ] #3 Test with missing headers
- [ ] #4 Test with unknown hostname
- [ ] #5 Use axum-test for handler testing
- [ ] #6 Mock database responses
<!-- AC:END -->
