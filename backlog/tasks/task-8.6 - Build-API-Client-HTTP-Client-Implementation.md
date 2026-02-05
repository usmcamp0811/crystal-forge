---
id: TASK-8.6
title: Build API Client - HTTP Client Implementation
status: To Do
assignee: []
created_date: '2026-02-05 14:15'
labels:
  - ui
  - api
  - backend
dependencies:
  - TASK-8.5
parent_task_id: TASK-8
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Implement the HTTP client using reqwest for API calls.

Steps:
1. Create src/api/client.rs
2. Define CrystalForgeClient struct with base_url and http_client
3. Implement helper methods: get(), post(), handle_response()
4. Implement API methods: get_dashboard_summary(), list_systems(), get_system(), etc.
5. Add timeout configuration (30s default)
6. Add error handling with anyhow/thiserror
7. Write unit tests for client creation

Expected: Client compiles, basic tests pass
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 CrystalForgeClient struct implemented
- [ ] #2 All API methods defined
- [ ] #3 Error handling in place
- [ ] #4 Unit tests pass
<!-- AC:END -->
