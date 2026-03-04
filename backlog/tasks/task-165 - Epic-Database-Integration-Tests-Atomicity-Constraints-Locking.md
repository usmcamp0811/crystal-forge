---
id: TASK-165
title: 'Epic: Database Integration Tests - Atomicity, Constraints, Locking'
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
Database integration tests needed to verify atomicity, constraints, locking, and "only API writes" rule.

## Goals
- Atomic claim + lease semantics tests (exclusive claim, lease expiration, lease renewal, max concurrent)
- Uniqueness + idempotency via DB constraints (cache_push_job, GC root lifecycle)
- Ordering guarantees (eval_queue_position, cache_push_job created_at)

## Scope
Tests requiring real Postgres, using Testcontainers or local dev DB.

## Release Blockers
- Atomic claim + lease expiry reclaim tests
- Complete/fail/log idempotency at DB level
- GC root lifecycle

## Acceptance Criteria
- [ ] Atomic claim is exclusive (20 concurrent claimers, each job claimed at most once)
- [ ] Lease expiration makes job reclaimable
- [ ] Lease renewal extends lease
- [ ] Max concurrent jobs per builder enforced
- [ ] Unique cache_push_job per derivation/store_path (DB constraint)
- [ ] Build job completion inserts exactly one GC root record
- [ ] Cache push completion deletes the right GC root
- [ ] Eval worker selects next pending commit respects eval_queue_position
- [ ] Cache push worker selects oldest pending first
<!-- SECTION:DESCRIPTION:END -->
