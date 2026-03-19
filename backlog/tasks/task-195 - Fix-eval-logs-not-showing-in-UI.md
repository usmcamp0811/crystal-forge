---
id: TASK-195
title: Fix eval logs not showing in UI
status: Review
assignee: []
created_date: '2026-03-19 00:46'
updated_date: '2026-03-19 03:14'
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

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
LOCK: Claude (OpenCode) on gray in ~/code/crystal-forge/TASK-195-fix-eval-logs-ui

## Implementation Complete

### Root Cause Found
WebSocket channels and log history were cleaned up immediately after evaluation completed. Late-connecting clients (which is always the case - users open UI after eval) had no history to replay.

### Solution
Delayed cleanup by 10 minutes using tokio::spawn. Keeps both broadcast channel and history available for late connections.

### Testing
Added automated WebSocket test to web-ui check:
- Creates flake/commit, triggers eval, waits for completion
- THEN connects WebSocket (late connection scenario)  
- Verifies logs are received (validates history replay)

### MR Created
MR !171: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/171

Ready for review and testing!

### Test Fix Applied

CI tests failed because eval_channel_fanout_and_cleanup test expected immediate cleanup.

Fixed by updating test to verify channels/history remain available after cleanup is called (correct behavior for delayed cleanup).

Commit: 806f1080 - Pushed to MR !171

CI should now pass!

## Latest Update (2026-03-18)

Pushed commit f35ad618 to fix WebSocket test authentication issue.

The web-ui check was failing because the test tried to register a new user, but the integration test already created an admin user. Fixed by reusing existing credentials (admin@example.com / adminpass).

Waiting for CI to pass. All other checks (nix-build, server-stack, integration-test) passed on previous commits.

## MR Description Updated (2026-03-18)

Updated MR !171 with proper description including:
- Summary of the problem and root cause
- Solution explanation (delayed cleanup)
- Changes made with line numbers
- Testing details (unit + integration tests)
- Tradeoffs and mitigation
- CI status: ✅ All checks passed

Ready for review and merge!
<!-- SECTION:NOTES:END -->
