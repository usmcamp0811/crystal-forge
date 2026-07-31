---
id: TASK-150
title: Close remaining test/spec gaps for TASK-145 through TASK-148
status: Cancelled
assignee: []
created_date: '2026-03-01 16:11'
updated_date: '2026-03-02 17:27'
labels:
  - security
  - follow-up
  - tests
  - documentation
dependencies: []
references:
  - TASK-145
  - TASK-146
  - TASK-147
  - TASK-148
priority: medium
ordinal: 2000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Follow-up hardening task to align merged security changes with strict acceptance criteria language from TASK-145/146/147/148.

Observed gaps after merge into dev:
- TASK-145: freshness-window replay protection implemented, but no nonce/dedup mechanism; tests for old timestamp and cross-endpoint signature reuse are not explicit.
- TASK-146: bounded log ingestion and retention implemented, but acceptance asked for explicit tests/docs coverage that is incomplete.
- TASK-147: atomic claim transaction implemented, but no dedicated concurrency/load test proving no over-commit under high contention.
- TASK-148: base64/length/key validation implemented, but acceptance asked for oversized-body 413 test and explicit docs wording.

Goal: add missing tests/docs (and nonce dedup if still required by policy) or update acceptance criteria to reflect accepted design.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Decide and document whether nonce dedup is required beyond freshness-window replay protection
- [ ] #2 Add explicit tests for TASK-145 replay and cross-endpoint signature binding
- [ ] #3 Add/adjust tests and docs for TASK-146 log limits and authorization behaviors
- [ ] #4 Add dedicated contention test for TASK-147 concurrency limit enforcement
- [ ] #5 Add explicit oversized-body validation test/docs coverage for TASK-148 or adjust accepted contract
<!-- AC:END -->
