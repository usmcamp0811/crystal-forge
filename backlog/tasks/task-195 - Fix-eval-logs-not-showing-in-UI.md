---
id: TASK-195
title: Fix eval logs not showing in UI
status: To Do
assignee: []
created_date: '2026-03-19 00:46'
labels:
  - bug
  - ui
  - websocket
  - logging
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

Evaluation logs are not appearing in the UI even though evaluations are running and being logged in the server process-compose output. The WebSocket connection is established but logs are not being streamed to the frontend.

## Current Behavior

From process-compose logs:
```
2026-03-19T00:09:07.826127Z  INFO crystal_forge::server: 📌 Found 5 pending commits
2026-03-19T00:09:07.829459Z  INFO crystal_forge::models::evaluate_with_policies: 🚀 Running: nix-eval-jobs for all with 6 policies
2026-03-19T00:09:07.854339Z ERROR crystal_forge::models::evaluate_with_policies: nix-eval-jobs stderr: error: undefined variable 'config'
2026-03-19T00:09:07.858649Z ERROR crystal_forge::server: ❌ Failed to evaluate commit 60c9a2dfbc763e30ee9664250e200794b1dc0d09
2026-03-19T00:10:21.418825Z  INFO crystal_forge::handlers::api::commits: WebSocket connection established for commit 1 evaluation
2026-03-19T00:10:21.418825Z  INFO crystal_forge::handlers::api::commits: WebSocket connection closed for commit 1 eval
```

WebSocket connections are being established, but the UI shows no logs.

## Expected Behavior

- Eval logs should stream in real-time to the UI
- Both INFO and ERROR logs should be visible
- WebSocket connection should stream logs continuously during eval
- Failed evals should show error messages in the UI

## Goal

Fix the log streaming from backend to frontend so users can see evaluation progress and errors in the UI.

## Impact Areas

- WebSocket handler for commit evaluation logs
- Frontend log display component
- Log streaming/buffering logic
- Error message propagation
<!-- SECTION:DESCRIPTION:END -->
