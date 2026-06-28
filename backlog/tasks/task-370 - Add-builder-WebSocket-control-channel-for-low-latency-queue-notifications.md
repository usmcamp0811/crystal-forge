---
id: TASK-370
title: Add builder WebSocket control channel for low-latency queue notifications
status: To Do
assignee: []
created_date: '2026-06-27 03:25'
updated_date: '2026-06-28 02:14'
labels:
  - builder
  - websocket
  - queue
  - reliability
  - infrastructure
milestone: Reliability
dependencies:
  - TASK-282
references:
  - TASK-282
  - TASK-154
  - packages/default/src/handlers/api/builders.rs
  - packages/default/src/bin/builder.rs
  - packages/default/src/builder/api_client.rs
  - packages/default/src/queue.rs
priority: medium
ordinal: 1000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

Builders currently rely on periodic polling (`GET /api/v1/builders/:id/next-job`) plus heartbeat to discover new work and report liveness. This is safe but can delay job pickup, cancellation/drain responses, and future builder control messages until the next poll/heartbeat interval.

## Goal

Add a lightweight authenticated builder WebSocket control channel so the server can push low-latency notifications to connected builders while preserving the existing heartbeat and `/next-job` claim API as the source of truth and fallback path.

## Non-Goals

- Replacing builder heartbeats
- Replacing the existing atomic `/next-job` job-claim API
- Treating WebSocket disconnect alone as proof that a builder process is dead
- Re-architecting the build scheduler or distributed coordination model
- Streaming build logs; TASK-154 already covers build log WebSocket streaming

## Architectural Constraints

- Heartbeat remains the authoritative liveness timeout for recovery decisions.
- `/next-job` remains the only path that atomically claims queued jobs.
- The WebSocket channel is advisory/control-plane only: notifications wake builders up, but do not assign jobs directly.
- Polling remains available as a fallback when the socket is disconnected or unsupported.
- Builder authentication must reuse existing builder identity/key patterns or an equivalently safe challenge/signature flow.
- Avoid hidden global mutable state; connection tracking should be explicit, scoped, and concurrency-safe.
- UI/business logic boundaries are not involved unless a future task surfaces connection state in the UI.

## Impact Areas

- Builder API handlers/routes
- Builder API client
- Builder process polling loop
- Queue notification plumbing
- Builder liveness/recovery interaction
- Tests for auth, reconnect/fallback, and no duplicate claims

## Risk Level

Medium-high: this touches server-builder coordination. The safe design keeps WebSocket notifications advisory and preserves existing atomic claim and heartbeat semantics to avoid duplicate execution or premature recovery.

## Dependencies

- TASK-282 should remain the baseline recovery behavior: heartbeat/liveness recovery must continue to work when the socket is absent or disconnected.

## Verification Plan

- Unit tests for connection registry/notification behavior if a registry abstraction is added.
- Handler/client tests for builder WebSocket authentication failure and success where practical.
- Targeted integration or async tests showing:
  - connected builder receives a queue notification and calls `/next-job`
  - disconnected builder still falls back to polling
  - socket disconnect alone does not requeue in-flight jobs before heartbeat timeout
  - two builders notified concurrently still cannot claim the same job because `/next-job` remains atomic
- Run `nix develop -c cargo fmt --manifest-path packages/default/Cargo.toml -- --check` or changed-file rustfmt equivalent.
- Run `nix develop -c env SQLX_OFFLINE=true cargo check --manifest-path packages/default/Cargo.toml`.
- Run relevant targeted cargo tests for builder API/client/queue behavior.
- Run a scoped Nix build/check if affected interfaces require it.

## Proposed Approach

1. Add an authenticated builder control WebSocket endpoint, e.g. `/api/v1/builders/:id/control`.
2. Track active control-channel connections per builder in an explicit server-side registry owned by application state.
3. Bridge existing queue notifications to connected builders by sending advisory messages such as `queue_available`, `drain_requested`, or `cancel_requested` where currently supported.
4. Update the builder process to race its existing poll interval with socket notifications; on notification, call existing `/next-job` rather than accepting an assigned job over the socket.
5. Keep heartbeat and stale-builder recovery unchanged; use socket state only for low-latency hints/observability, not final liveness decisions.
6. Preserve polling fallback and reconnect with bounded backoff.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Builder opens an authenticated WebSocket control channel to the server in API mode while retaining current heartbeat behavior.
- [ ] #2 Server can send an advisory queue-available notification to connected builders without assigning jobs directly over the socket.
- [ ] #3 Builder responds to queue notifications by calling the existing atomic /next-job claim endpoint.
- [ ] #4 Polling remains as a fallback when the control socket is disconnected or unavailable.
- [ ] #5 Socket disconnect alone does not mark a builder dead or requeue building jobs before heartbeat timeout recovery applies.
- [ ] #6 Concurrent notifications to multiple builders cannot create duplicate execution for a single build job.
- [ ] #7 Targeted tests cover authentication, notification-triggered claim, polling fallback, disconnect behavior, and duplicate-claim prevention.
<!-- AC:END -->
