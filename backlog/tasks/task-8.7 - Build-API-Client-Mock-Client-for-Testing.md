---
id: TASK-8.7
title: Build API Client - Mock Client for Testing
status: To Do
assignee: []
created_date: '2026-02-05 14:15'
labels:
  - ui
  - api
  - testing
dependencies:
  - TASK-8.5
parent_task_id: TASK-8
priority: high
milestone: m-3
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Create a mock API client that returns fake data for development.

Steps:
1. Create src/api/mock.rs
2. Define MockClient struct
3. Implement same methods as real client but return hardcoded data
4. Create realistic test data: 2-3 systems, builds, flakes
5. Use chrono::Utc::now() for timestamps
6. Add to src/api/mod.rs exports
7. Document how to use mock vs real client

Expected: Frontend can develop without backend being ready
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 MockClient implements all API methods
- [ ] #2 Realistic test data provided
- [ ] #3 Documentation on usage
<!-- AC:END -->
