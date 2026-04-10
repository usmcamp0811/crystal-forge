---
id: TASK-234
title: Investigate and fix live build log WebSocket streaming regression
status: To Do
assignee: []
created_date: '2026-04-01 02:07'
updated_date: '2026-04-10 02:38'
labels:
  - bug
  - websocket
  - build-logs
  - backend
  - ui
  - sprint-ready
milestone: m-12
dependencies: []
priority: high
ordinal: 1000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem Statement
Live build log streaming in the Builds detail view appears non-functional in production usage. The feature is implemented (TASK-154), but users are not receiving real-time log updates while builds run.

## Goal
Identify root cause(s) and restore reliable end-to-end live build log streaming (builder -> server WebSocket endpoint -> UI subscriber) with clear diagnostics and regression coverage.

## Non-Goals
- No redesign of build queue UX beyond what is necessary to display live logs.
- No unrelated auth/RBAC refactors outside stream access path.
- No protocol redesign unless required to fix compatibility breakage.

## Scope
- Reproduce failure path with deterministic local/integration scenario.
- Verify server route + auth behavior for `/api/v1/build-jobs/:job_id/logs/stream`.
- Verify builder publish path and frame format handling.
- Verify UI subscription path (job_id binding, connection lifecycle, rendering updates).
- Fix minimal root cause(s) and add regression tests/checks.
- Add operational troubleshooting notes (expected status codes, key log lines, proxy requirements).

## Architectural Constraints
- Preserve existing separation: builder emit, API stream handling, UI presentation.
- Keep security boundaries intact (viewer/builder auth rules on stream endpoint).
- Avoid introducing hidden global mutable state in stream/channel lifecycle.

## Verification Plan
- Backend test(s) for stream auth/connection behavior (viewer allowed, unauthorized denied).
- Integration/feature check proving log lines emitted by builder are received by UI stream consumer for active job.
- Web UI check updated/asserted if needed to validate live log activity signal.
- Targeted commands:
  - `nix develop -c env SQLX_OFFLINE=true cargo test --package crystal-forge <stream-tests>`
  - `nix develop -c cargo test --package crystal-forge-ui <build-log-stream-tests>`
  - `nix build .#checks.x86_64-linux.server --no-link`
  - `nix build .#checks.x86_64-linux.web-ui --no-link`

## Impact Areas
- `packages/default/src/handlers/api/builders.rs`
- `packages/default/src/bin/server.rs`
- `packages/default/src/bin/builder.rs` (if publisher-side fix needed)
- `packages/web-ui/src/hooks/websocket.rs`
- `packages/web-ui/src/components/builds/build_detail_pane.rs`
- Related tests/checks

## Risk Level
High (observability during builds is operationally critical).

## Dependencies
- Builds pipeline and builder heartbeat/job assignment path must be functioning for end-to-end validation.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 For an active build with a valid job_id, the UI connects to `/api/v1/build-jobs/:job_id/logs/stream` and displays new log lines in near real time.
- [ ] #2 Stream endpoint enforces intended authz behavior (authorized viewer/builder allowed; unauthorized denied) with tests.
- [ ] #3 If WS connection cannot be established, UI shows actionable state/error rather than silent non-updating logs.
- [ ] #4 Regression coverage exists for the root-cause failure mode found in this task.
- [ ] #5 Task notes document root cause, fix summary, and operator verification steps (including expected network/log signals).
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Marked for closure by maintainer confirmation: live build log WebSocket streaming is currently working in production and this task is no longer needed as a standalone work item.
<!-- SECTION:NOTES:END -->
