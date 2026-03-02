---
id: TASK-154
title: Implement WebSocket-based real-time build log streaming
status: In Progress
assignee: []
created_date: '2026-03-02 03:36'
updated_date: '2026-03-02 04:47'
labels:
  - enhancement
  - builder
  - websocket
  - logs
  - ui
dependencies: []
references:
  - >-
    packages/default/src/derivations/build.rs - run_streaming_build() captures
    output
  - >-
    packages/default/src/bin/builder.rs - execute_build_job() needs WebSocket
    client
  - packages/web-ui/src/components/builds/build_detail_pane.rs - log display UI
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

Currently, build logs are only sent to the server at lifecycle points (build start, complete, failure). For long-running builds (Firefox, Chrome, etc. that can take 1+ hours), users need to see real-time Nix build output to monitor progress.

The current implementation sends high-level status updates but doesn't stream the actual `nix-store` stdout/stderr output in real-time.

## Goal

Implement WebSocket-based log streaming so users can watch build progress in real-time as Nix downloads dependencies, compiles code, and runs build steps.

## Current State

- Build execution works and completes successfully
- Logs are stored in database after completion
- UI has log display components ready
- `run_streaming_build()` in `derivations/build.rs` already captures stdout/stderr line-by-line
- The captured logs currently only go to tracing/logs, not to the API

## Proposed Solution

### Architecture

```
Builder Process
  ├─ Captures nix-store stdout/stderr (already done)
  ├─ Sends lines via WebSocket to server
  └─ Falls back to batched HTTP POST if WebSocket unavailable

Server
  ├─ WebSocket endpoint: /api/v1/builders/:id/jobs/:job_id/logs/stream
  ├─ Stores logs in database (append)
  └─ Broadcasts to UI clients watching the job

UI
  ├─ Connects to WebSocket when viewing build details
  ├─ Streams logs in real-time
  └─ Auto-scrolls / follows mode
```

## Non-Goals

- Video/terminal emulation
- Log replay (just show current/final state)
- Log filtering in UI (can add later)

## Impact

- **Users**: Can monitor long builds instead of waiting blindly
- **Debugging**: See build failures as they happen
- **Operations**: Monitor builder health in real-time
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Builder streams stdout/stderr from nix-store to server via WebSocket
- [ ] #2 Server WebSocket endpoint accepts and stores log lines
- [ ] #3 Logs are persisted to build_jobs.logs in database
- [ ] #4 UI connects to WebSocket and displays logs in real-time
- [ ] #5 System falls back to HTTP batching if WebSocket fails
- [ ] #6 Auto-scroll / follow mode works in UI
- [ ] #7 Existing build execution continues to work unchanged
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
**Started work**: 2026-03-02
**Worktree**: TASK-154-websocket-build-logs
**Branch**: TASK-154-websocket-build-logs (from dev)
<!-- SECTION:NOTES:END -->

## Definition of Done
<!-- DOD:BEGIN -->
- [ ] #1 WebSocket server endpoint implemented and tested
- [ ] #2 Builder sends logs via WebSocket during build
- [ ] #3 UI displays streaming logs with auto-scroll
- [ ] #4 Integration test verifies end-to-end log flow
- [ ] #5 Documentation updated with WebSocket architecture
<!-- DOD:END -->
