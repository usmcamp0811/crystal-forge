---
id: TASK-154
title: Implement WebSocket-based real-time build log streaming
status: In Progress
assignee: []
created_date: '2026-03-02 03:36'
updated_date: '2026-03-02 05:23'
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

## Progress Update

### Server-Side WebSocket Endpoint ✅
- Added axum WebSocket feature to Cargo.toml
- Created `/api/v1/build-jobs/:job_id/logs/stream` endpoint
- Supports dual message types:
  1. **Plain text** = Build log lines (stored in database)
  2. **JSON {cpu_percent, ram_used_mb, ram_total_mb, timestamp}** = System metrics (broadcast only, not stored)
- Handles WebSocket lifecycle (ping/pong, close, errors)
- Route registered in server.rs

### Dependencies Updated ✅
- Enabled `ws` feature on axum 0.7
- Added tokio-tungstenite 0.24.0 (via Cargo.lock update)
- Build in progress...

### Next Steps
1. ✅ Complete build verification
2. ⏳ Update builder to send logs via WebSocket
3. ⏳ Add system metrics collection in builder
4. ⏳ Update UI to display streaming logs + metrics
5. ⏳ Add HTTP fallback for reliability
6. ⏳ Test end-to-end

## MAJOR PROGRESS UPDATE (2026-03-02)

### ✅ Completed

1. **Server WebSocket Endpoint** (commit 82f827d7)
   - Added `/api/v1/build-jobs/:job_id/logs/stream` endpoint
   - Dual message type support: plain text logs + JSON metrics
   - WebSocket lifecycle handling (ping/pong, close, errors)
   - Route registered in server.rs

2. **Builder WebSocket Client** (commit 885c7761)
   - WebSocket connection established before build starts
   - BuilderApiClient::create_log_stream() implemented
   - Sends log lines as plain text messages
   - Spawns metrics collection task (2-second interval)
   - Sends CPU/RAM metrics as JSON: {cpu_percent, ram_used_mb, ram_total_mb, timestamp}
   - Falls back to HTTP POST if WebSocket unavailable
   - Metrics task cleaned up on build completion
   - Added tokio-tungstenite dependency

3. **UI WebSocket Client** (commit 5f4965f2)
   - Added WebSocket features to web-sys (WebSocket, MessageEvent, CloseEvent, ErrorEvent, BinaryType)
   - Created hooks/websocket module with connection hooks
   - Implemented use_websocket_build_stream hook (combined logs + metrics)
   - ConnectionState enum: Disconnected, Connecting, Connected, Error
   - BuildDetailPane updated to use WebSocket streaming when job_id available
   - Live connection status indicator (color-coded dot)
   - Real-time CPU/RAM metrics display (updates every 2s from builder)
   - Auto-connect on component mount
   - Falls back to mock data if no job_id

### 🔨 Architecture Implemented

```
Builder → WebSocket → Server → UI
   │                      │
   ├─ Plain text logs ────┼─→ Database (build_jobs.logs)
   └─ JSON metrics ───────┼─→ Broadcast only (not stored)
                          └─→ All connected clients
```

### ⏳ Remaining Work

1. **End-to-End Testing**
   - Manual test: Start a real build, watch logs stream in UI
   - Verify metrics update every 2 seconds
   - Test HTTP fallback when WebSocket disabled
   - Verify logs persist to database correctly

2. **Polish**
   - Add auto-scroll / follow mode to UI (may already work via Dioxus signals)
   - Test with multiple clients viewing same build
   - Consider adding reconnection logic (currently auto-connects on mount)

3. **Documentation**
   - Update acceptance criteria checkboxes
   - Document WebSocket message format
   - Add architecture diagram to docs

### 🎯 Status

**Current State**: Core implementation complete, ready for testing
**Next Action**: Manual end-to-end test with real build
<!-- SECTION:NOTES:END -->

## Definition of Done
<!-- DOD:BEGIN -->
- [ ] #1 WebSocket server endpoint implemented and tested
- [ ] #2 Builder sends logs via WebSocket during build
- [ ] #3 UI displays streaming logs with auto-scroll
- [ ] #4 Integration test verifies end-to-end log flow
- [ ] #5 Documentation updated with WebSocket architecture
<!-- DOD:END -->
