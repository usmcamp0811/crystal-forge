---
id: TASK-161
title: Address MR !150 pre-merge WebSocket hardening
status: Backlog
assignee: []
created_date: '2026-03-02 17:14'
labels: []
dependencies:
  - TASK-154
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Apply reviewer-requested hardening and verification for WebSocket build/eval streaming before merging MR !150.

## Problem
TASK-154 is in Review, but merge is blocked by concerns around authorization, resource safety, framing correctness, and fallback correctness.

## Goal
Close merge blockers for MR !150 with focused fixes and automated coverage where possible.

## Non-Goals
- New UI feature work (separate from merge-blocker hardening)
- Broad refactors unrelated to streaming path

## Scope
- Enforce authorization parity on WS endpoints
- Add explicit WS message discrimination to avoid log/JSON ambiguity
- Verify/cap buffering and cleanup behavior for broadcast channels
- Validate WS->HTTP fallback behavior for duplication/gaps
- Add handler-level tests for build/eval WS behavior

## References
- MR !150
- TASK-154 review notes (2026-03-02 merge-readiness section)
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 WS endpoints enforce authorization equivalent to corresponding REST resource access controls
- [ ] #2 Builder-side ingest endpoint is restricted to builder/agent identity and rejects unauthorized clients
- [ ] #3 WS payload format uses an explicit message type discriminator so log lines cannot be misclassified as metrics
- [ ] #4 Eval broadcast channels are cleaned up on completion and error paths, with bounded buffering behavior documented/verified
- [ ] #5 Build log ingestion path enforces a reasonable max frame/message size or line length guard
- [ ] #6 WS->HTTP fallback behavior is verified to avoid duplicate persisted lines and avoid dropped tail lines
- [ ] #7 At least one automated test covers build-log WS ingest/storage semantics
- [ ] #8 At least one automated test covers eval WS fanout to multiple clients and channel cleanup
<!-- AC:END -->
