---
id: TASK-168
title: 'Epic: Failure Injection / Chaos Tests'
status: Backlog
assignee: []
created_date: '2026-03-04 03:09'
labels: []
milestone: m-15
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem
Failure injection and chaos tests needed to catch real bugs that only appear under failure conditions.

## Goals
- Worker crashes mid-step (builder, cache worker, eval worker)
- Network / API errors (intermittent 500 on log upload, agent heartbeat failures)
- Verify recovery semantics work correctly

## Scope
Simulate crashes, network failures, and verify state recovery.

## Release Blockers
- Builder crashes mid-build (lease expiry, job reclaimable, logs retained)
- Cache worker crashes mid-push (job retried safely, gc_root handled correctly)
- Agent heartbeat fails temporarily (no state corruption, eventual convergence)

## Acceptance Criteria
- [ ] Builder crashes mid-build: lease expires, job reclaimable, logs retained
- [ ] Cache worker crashes mid-push: job retried safely, no gc_root deletion until completion
- [ ] Eval worker crashes after setting commit in_progress: recovers correctly
- [ ] Builder gets intermittent 500 on log upload: retries handled, job can complete
- [ ] Agent heartbeat fails temporarily: no state corruption, eventual convergence
<!-- SECTION:DESCRIPTION:END -->
