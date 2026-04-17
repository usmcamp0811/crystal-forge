---
id: TASK-233
title: Queue build jobs incrementally as each flake configuration finishes evaluation
status: Review
assignee: []
created_date: '2026-04-01 01:34'
updated_date: '2026-04-02 00:15'
labels:
  - eval
  - build-queue
  - backend
  - sprint-ready
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem Statement
Current behavior queues build jobs only after the entire commit evaluation finishes. For large flakes, this delays build start for early-successful systems and increases end-to-end latency.

## Goal
Enable incremental build queueing so each nixosConfiguration can be queued for build as soon as its evaluation + policy checks succeed, without waiting for full-commit evaluation completion.

## Non-Goals
- No change to derivation policy semantics (strict policy failures still do not queue).
- No change to builder claim/priority algorithm beyond what is required for safe incremental inserts.
- No schema redesign unless strictly required for idempotency/locking safety.
- No change to user-facing build status taxonomy.

## Scope
- Introduce per-derivation build job enqueue path invoked during evaluation stream processing when a derivation reaches DryRunComplete and passes queue eligibility.
- Preserve idempotency guarantees (no duplicate build_jobs for same derivation) under retries/restarts/concurrent evaluators.
- Keep existing post-eval bulk queue step either as fallback/backstop or remove it if provably redundant; document chosen approach.
- Update relevant logs/telemetry so operators can observe incremental queueing progress.

## Architectural Constraints
- Keep evaluation orchestration in server/model layers; queue persistence logic remains in query layer.
- Do not move business logic into UI.
- Maintain clear transaction/consistency boundaries and avoid hidden global mutable state.

## Verification Plan
- Unit/integration test: when first system in a multi-system commit evaluates successfully, a build job exists before evaluation of the final system completes.
- Idempotency test: repeated eval events/retries do not create duplicate build jobs.
- Failure-path test: systems with policy failure or eval error are not queued.
- Restart/recovery test (or deterministic simulation): incremental queueing remains correct after evaluator interruption.
- Targeted checks:
  - `nix develop -c env SQLX_OFFLINE=true cargo test --package crystal-forge <incremental-queue-tests>`
  - `nix develop -c env SQLX_OFFLINE=true cargo check --package crystal-forge`
  - `nix build .#checks.x86_64-linux.server --no-link`

## Impact Areas
- `packages/default/src/models/evaluate_with_policies.rs`
- `packages/default/src/server/mod.rs`
- `packages/default/src/queries/build_jobs.rs` (or derivation queue query module)
- Related tests under `packages/default/src/**`

## Risk Level
High (queueing semantics affect build ordering, duplicate prevention, and recovery behavior).

## Dependencies
- Coordinate with restart/resume behavior tracked in TASK-161 to avoid regressions in partially-evaluated commits.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 For a commit with multiple nixosConfigurations, at least one build job is created before full commit evaluation completes when an early config succeeds.
- [ ] #2 Each eligible derivation is queued at most once (no duplicate build_jobs) across retries/restarts/concurrent processing.
- [ ] #3 Configurations failing eval or strict policy checks are not queued incrementally.
- [ ] #4 Incremental queueing behavior is covered by automated tests for success, failure, and idempotency scenarios.
- [ ] #5 Task notes document whether post-eval bulk queueing remains as fallback and the exact ordering/idempotency guarantees.
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Moved Backlog -> To Do per explicit human request in chat.

LOCK: claude-sonnet-4-6 on reckless in /home/mcamp/code/crystal-forge/TASK-233-incremental-build-queue

MR: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/203

Commit: 5f373cd8

server nix check passes. 4 unit tests pass. sqlx offline cache updated.

Post-eval create_build_jobs_for_commit retained as idempotent backstop. NOT EXISTS guard prevents duplicates in both paths.

Closed during backlog cleanup per maintainer direction (MR merged/closed). Task archived from active review queue.
<!-- SECTION:NOTES:END -->
