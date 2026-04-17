---
id: TASK-166
title: 'Epic: Service Integration Tests - API + Workers with Fake Nix'
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
Service integration tests needed to verify the full pipeline behavior with fake executors (no real Nix).

## Goals
- Commit ingestion → eval queue behavior
- Eval loop behavior (mark in_progress, complete, partial results, retry)
- Build queue behavior (lease, streaming logs, completion, failure, retry)
- Cache push worker behavior (idempotent push, failure, backoff)

## Scope
API + workers running, but using fake nix-eval-jobs, nix build, nix copy.

## Release Blockers
- Complete/fail/log idempotency
- Partial results + policy gating

## Acceptance Criteria
- [ ] Webhook creates commit and enqueues eval (idempotent)
- [ ] Eval marks commit in_progress then complete
- [ ] Partial eval results: build_jobs only for DryRunComplete
- [ ] Eval retry path: no duplicate derivations/build_jobs
- [ ] Builder fetches job, streams logs, completes (creates cache_push_job + GC root)
- [ ] Builder fails job and retry happens correctly
- [ ] Builder attempts to complete after losing lease (rejected)
- [ ] Idempotent push "already exists" treated as success
- [ ] Push failure and backoff respected
<!-- SECTION:DESCRIPTION:END -->
