---
id: TASK-161
title: Add resumable per-system eval for commits after restart
status: Backlog
assignee: []
created_date: '2026-03-03 04:49'
labels:
  - eval-queue
  - resilience
  - nix-eval-jobs
dependencies: []
references:
  - TASK-160
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem
When evaluating a commit with many `nixosConfigurations` (for example 35 systems), a server restart currently causes the evaluation to restart from scratch. This re-evaluates systems that were already processed and delays build queue progress.

## Desired Outcome
Support partial/resumable evaluation for a commit so already-evaluated systems are skipped after restart.

### Goal
- Allow evaluation to target a subset of systems in a flake (if supported directly by `nix-eval-jobs` invocation or by wrapper logic).
- Persist per-system evaluation completion state as systems finish.
- On restart/retry for the same commit, only evaluate remaining systems.
- Keep event-driven behavior: each completed system should be eligible to enqueue build work immediately (without waiting for full-commit evaluation).

### Notes
- This should preserve current policy-check behavior and queue semantics.
- Investigate whether `nix-eval-jobs` can natively limit to explicit attrs/systems; if not, implement filtering in the generated expression/wrapper.
- Ensure idempotency when re-processing commit eval state and build queue enqueueing.
<!-- SECTION:DESCRIPTION:END -->
