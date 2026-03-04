---
id: TASK-163
title: 'Epic: Cross-Cutting Test Infrastructure & Invariants'
status: Backlog
assignee: []
created_date: '2026-03-04 03:08'
labels: []
milestone: m-15
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem
Need reusable test helpers and assertions for cross-cutting invariants that apply across all test layers.

## Goals
- Implement single-writer rule verification (DB role permissions, network isolation)
- Implement valid state machine transition assertions
- Implement at-least-once / idempotency helpers
- Implement monotonicity helpers (retry_count, timestamps)
- Implement ordering helpers (eval_queue_position, created_at)

## Scope
Create shared test utilities in src/test_utils/ that can be used by unit, integration, and E2E tests.

## Acceptance Criteria
- [ ] Single-writer rule test helper exists and verifies API-only DB access
- [ ] State machine transition assertion helper rejects illegal transitions
- [ ] Idempotency test helper can verify duplicate requests produce same state
- [ ] Monotonicity helpers verify retry_count and timestamp ordering
- [ ] Ordering helpers verify eval_queue_position and created_at ordering
<!-- SECTION:DESCRIPTION:END -->
