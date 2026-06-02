---
id: TASK-289
title: >-
  Implement eval log streaming with database persistence and historical
  retrieval
status: Done
assignee: []
created_date: '2026-05-07 14:21'
updated_date: '2026-06-02 03:45'
labels:
  - feature
  - backend
  - frontend
  - evaluations
  - logging
milestone: Evaluations UX
dependencies: []
modified_files:
  - packages/default/migrations/0121_add_eval_logs_table.sql
  - packages/default/src/queries/eval_logs.rs
  - packages/default/src/models/evaluate_with_policies.rs
  - packages/default/src/handlers/api/commits.rs
  - packages/default/src/handlers/api/mod.rs
  - packages/default/src/server.rs
  - packages/web-ui/src/api/models.rs
  - packages/web-ui/src/api/client.rs
  - packages/web-ui/src/components/eval_log_modal.rs
  - packages/web-ui/src/views/evaluations.rs
priority: high
ordinal: 0
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Add database persistence for evaluation logs with REST API for historical retrieval and full UI integration for real-time + historical log viewing.

## Context
Evaluation logs currently only stream via WebSocket but are lost after the evaluation completes. Users cannot view historical logs or download them.

## Solution
Three-phase implementation:
1. Backend persistence: Create eval_logs table, CRUD operations, REST endpoint
2. Frontend API: Add DTO and client function
3. UI integration: Wire up modal to fetch/merge historical + live logs, enable history tab, add download

## Implementation
- Migration 0121: eval_logs table with foreign key cascade
- queries/eval_logs.rs: insert, batch insert, fetch, delete operations
- Modified eval worker to broadcast AND persist logs
- REST endpoint: GET /api/v1/commits/:id/eval/logs
- Enhanced EvalLogModal to detect status and fetch historical logs
- Added download functionality using browser Blob API
- Enabled history tab logs button
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Logs stored in database with timestamp, sequence, level, message
- [ ] #2 Foreign key cascade deletes logs when commit deleted
- [ ] #3 REST endpoint returns logs for a commit_id
- [ ] #4 Eval worker persists logs alongside WebSocket broadcast
- [ ] #5 UI detects in-progress vs completed evaluations
- [ ] #6 UI fetches historical logs for completed evaluations
- [ ] #7 History tab logs button enabled and functional
- [ ] #8 WebSocket streaming works for in-progress evaluations
- [ ] #9 Concise/verbose toggle works
- [ ] #10 Log modal shows real-time + historical logs
- [ ] #11 Connection status indicator works
- [ ] #12 Download logs button functional
<!-- AC:END -->
