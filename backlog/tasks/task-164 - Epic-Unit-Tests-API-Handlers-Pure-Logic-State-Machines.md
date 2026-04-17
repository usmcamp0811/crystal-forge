---
id: TASK-164
title: 'Epic: Unit Tests - API Handlers, Pure Logic, State Machines'
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
Unit tests needed for API request handlers, pure logic (policy evaluation), and state machines - fast tests without Postgres or Nix.

## Goals
- API handler idempotency and validation tests (logs append, job complete, job fail, next-job, heartbeat, agent state)
- Pure logic tests (eval policy gate, deployable definition, backoff function)
- State machine transition tests (Commit, Derivation status transitions)

## Scope
Fast unit tests in Rust using mocks, no external dependencies.

## Release Blockers
These are foundational - blocking integration and E2E tests.

## Acceptance Criteria
- [ ] POST /builders/:id/jobs/:id/logs appends safely (deduplication, ordering)
- [ ] POST /builders/:id/jobs/:id/complete is idempotent (no duplicate cache_push_job)
- [ ] POST /builders/:id/jobs/:id/fail increments retry correctly
- [ ] GET /builders/:id/next-job respects max_concurrent_jobs
- [ ] Agent heartbeat responds with desired_target correctly
- [ ] POST /agent/state is idempotent
- [ ] Eval policy gate returns correct allowed/denied + reason
- [ ] Deployable definition truth table tested
- [ ] Backoff function monotone increasing, capped, jitter bounds verified
- [ ] Commit/Derivation state machine illegal transitions rejected
<!-- SECTION:DESCRIPTION:END -->
