---
id: TASK-174
title: Add deterministic mock eval/build dev mode for fast workflow validation
status: In Progress
assignee: []
created_date: '2026-03-04 23:28'
updated_date: '2026-03-07 03:16'
labels:
  - dev-experience
  - eval-queue
  - builder
  - testing
dependencies: []
priority: high
ordinal: 2000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem Statement
Eval and build phases are high-latency compared to UI iteration speed. This slows validation of queue behavior, ordering, logs, retries, and state transitions. We need a safe way to exercise the full process flow quickly in development without running real `nix-eval-jobs` and `nix build`.

## Goal
Introduce a deterministic **dev-only mock execution mode** for eval and build that simulates realistic timings, statuses, logs, and queue transitions, while preserving the same API/DB/event flow used in production.

## Non-Goals
- No production behavior changes when mock mode is disabled.
- No replacement of real eval/build paths; mock mode is additive and dev-only.
- No weakening of production safety checks.

## Proposed Approach
- Add a server config switch for execution mode:
  - `execution.mode = "real" | "mock"` (default `real`)
- Add a strict safety gate:
  - Mock mode only allowed in explicit dev environment (e.g., `server.dev_mode=true` + non-prod profile).
  - Server must hard-fail startup if `execution.mode=mock` in production profile.
- Implement mock adapters behind existing eval/build interfaces:
  - Mock eval emits per-system events/logs/policy outcomes with deterministic pseudo-random timing.
  - Mock build emits lease/heartbeat/log completion transitions and optional controlled failures/retries.
- Keep event-driven queue semantics intact:
  - Eval completion enqueues build jobs exactly as real mode does.
  - Ordering and reordering behavior can be validated end-to-end.
- Add observability marker in UI/API responses:
  - Clearly show `MOCK MODE` badge to prevent operator confusion.

## Verification Plan
- With `execution.mode=mock` in dev:
  - Queue ingest -> eval -> build transitions complete quickly.
  - Logs stream and status transitions mirror real flow shape.
  - Reordering queue changes next claimed eval/build as expected.
- With `execution.mode=real` (default):
  - Existing behavior unchanged.
- With production profile + `execution.mode=mock`:
  - Server startup fails with explicit error.

## Why Before TASK-173
This enables rapid, deterministic reproduction/validation for the eval-log-visibility and queue-ordering bugs in TASK-173, reducing cycle time and flakiness in verification.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A config toggle exists to switch execution mode between `real` and `mock`, defaulting to `real`.
- [ ] #2 Mock mode is blocked in production and startup fails if enabled outside approved dev context.
- [ ] #3 Mock eval and mock build paths drive the same queue/state APIs and DB transitions as real mode.
- [ ] #4 Mock mode emits realistic streaming logs/events for eval and build phases.
- [ ] #5 UI/API clearly indicates when mock mode is active.
- [ ] #6 A short developer guide documents how to enable/disable mock mode and expected behavior.
<!-- AC:END -->

## Definition of Done
<!-- DOD:BEGIN -->
- [ ] #1 Automated test(s) cover production guardrail (mock mode rejected in prod).
- [ ] #2 Automated test(s) cover core mock transition flow (pending -> in_progress -> complete/fail path).
- [ ] #3 Backlog task TASK-173 depends on this task so bug-fix validation uses mock mode.
<!-- DOD:END -->
