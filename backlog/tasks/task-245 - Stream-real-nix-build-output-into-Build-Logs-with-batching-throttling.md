---
id: TASK-245
title: Stream real nix build output into Build Logs with batching/throttling
status: Backlog
assignee: []
created_date: '2026-04-04 14:43'
labels:
  - builds
  - logs
  - ui
  - builder
  - observability
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

In API builder mode, the Build Logs panel in the web UI shows only a few milestone messages such as:
- `🔨 Starting build ...`
- `🔐 Signing derivation...`
- `📤 Queuing cache push job...`

But it does **not** show the actual `nix build` stdout/stderr output that operators expect during a running build.

Current behavior:
- `packages/default/src/bin/builder.rs` appends milestone logs to the job log stream / DB
- `packages/default/src/derivations/build.rs` reads real build stdout/stderr lines, but only sends them to tracing:
  - `info!("build stdout: {}", line)`
  - `debug!("build stderr: {}", line)`
- Those lines therefore appear in journald / server logs, not in the Build Logs UI

Operational impact:
- the build UI looks stuck or dead even while a build is actively progressing
- operators cannot prove that something is happening from the UI alone
- debugging requires shell access to builder/server logs instead of using Crystal Forge’s own build log view

## Goal

Make Build Logs useful for live troubleshooting by streaming real `nix build` output into the build job log stream that the UI already displays.

## Desired Outcome

During a running build, the Build Logs panel should show:
- milestone messages (existing behavior)
- real stdout/stderr emitted by the underlying nix build process
- live websocket updates while the build runs
- persisted log lines available after page refresh / after build completion

## Non-Goals

- No redesign of the Build Logs UI layout
- No raw byte-perfect log preservation guarantees beyond current storage limits
- No changes to TASK-239 scope except that this task should be compatible with it
- No attempt to stream *every* byte immediately if batching is needed for scale

## Scope

### 1. Forward build stdout/stderr into job logs

In `packages/default/src/derivations/build.rs`, `run_streaming_build()` currently reads stdout/stderr lines from the running child process.

Instead of only tracing them to journald, forward them into the build job log stream used by the UI.

Likely implementation options:
- Pass a log sink / callback into `run_streaming_build()` from `bin/builder.rs`
- Or pass `job_id`, `BuilderApiClient`, and optional websocket handle so `run_streaming_build()` can append logs directly

### 2. Batch / throttle log writes

Do **not** append one DB/websocket write per single line if avoidable.

Recommended default:
- accumulate stdout/stderr lines into a small buffer
- flush on either:
  - size threshold (e.g. 4-16 KB), or
  - time threshold (e.g. every 250-500 ms)
- reuse the existing websocket-first / HTTP-fallback path if possible

This keeps the UI feeling live without creating excessive DB write amplification.

### 3. Preserve milestone logs
n
Existing milestone logs from `bin/builder.rs` should remain; the real build output should appear in addition to them.

### 4. Keep persisted logs readable

The existing storage path (`build_jobs.logs`) and websocket stream should still produce readable, line-oriented logs in the UI.

### 5. Verify log limits still make sense

`append_job_logs_with_limits()` currently caps total stored logs. Confirm the new flow respects that cap gracefully.

## Architectural Constraints

- Prefer a single shared log path (websocket-first / HTTP fallback) rather than inventing a second parallel mechanism
- Avoid excessive per-line DB writes
- Keep the change scoped to build log transport, not UI redesign
- Preserve compatibility with current Build Logs websocket consumers in `packages/web-ui`

## Verification Plan

### Tier 0
- `nix develop -c env SQLX_OFFLINE=true cargo check --package crystal-forge`
- targeted tests for any new batching/flush helper
- verify milestone logs still appear in the final log stream

### Tier 1
- Start a real or mock build that emits multiple log lines over time
- Open the Build Logs panel in the UI
- Verify logs visibly stream beyond the single `Starting build` line
- Refresh the page and confirm streamed output is persisted
- Verify large/verbose builds still respect log-size guardrails

## Impact Areas

- `packages/default/src/derivations/build.rs`
- `packages/default/src/bin/builder.rs`
- possibly `packages/default/src/builder/api_client.rs`
- possibly `packages/default/src/handlers/api/builders.rs` / `queries/builders.rs` only if batching path requires adjustment
- `packages/web-ui/src/components/builds/build_detail_pane.rs` should be verified but may not need changes

## Risk Level

High

This touches build log transport for running builds, but the scope is isolated and high-value for operator UX.

## References

- `packages/default/src/derivations/build.rs`
- `packages/default/src/bin/builder.rs`
- `packages/default/src/builder/api_client.rs`
- `packages/default/src/handlers/api/builders.rs`
- `packages/default/src/queries/builders.rs`
- `packages/web-ui/src/components/builds/build_detail_pane.rs`
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 During a running build, the Build Logs panel shows real nix build stdout/stderr, not just milestone messages.
- [ ] #2 Real build output is streamed live to the existing build log websocket/UI path and remains persisted after refresh.
- [ ] #3 Log transport uses batching/throttling so normal builds do not generate one DB write per individual output line.
- [ ] #4 Existing milestone log messages remain visible in the combined log stream.
- [ ] #5 Targeted verification demonstrates a build with multiple emitted lines appearing progressively in the UI.
<!-- AC:END -->
