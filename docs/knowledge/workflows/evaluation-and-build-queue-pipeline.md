---
type: Workflow
title: "Evaluation and Build Queue Pipeline"
description: "Describes the two-stage commit pipeline (evaluation queue then build queue), its database fields, startup resets, and the single-active-evaluation invariant; open it when changing evaluation or build scheduling."
tags:
  - crystal-forge
  - workflow
  - evaluation
  - build
  - queue
implementation_status: implemented
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-03T22:57:15-05:00
sources:
  - id: origin
    resource: "Crystal Forge repository file docs/specs/00-system-overview.md at commit 3b23d36f"
    title: "Crystal Forge - System Overview"
---

# 2. Evaluation and Build Queue Pipeline

> **Status:** Split from the system overview. The unique index `idx_commits_single_in_progress` exists (`packages/default/crates/cf-server/migrations/0088_enforce_single_in_progress_eval.sql`, recreated in `0113_add_eval_cancellation_support.sql`). The routes `GET /build-queue` and `POST /builders/:id/pause` in "Key APIs" are verification candidates: the code registers `/api/v1/build-queue/reorder` (`packages/default/crates/cf-server/src/handlers/api/builders.rs`).

CF has a **two-stage pipeline** for processing flake commits: Evaluation → Build

## Stage 1: Evaluation Queue

When a new commit is detected:

1. Commit added to database with `evaluation_status = 'pending'`
2. Evaluation loop picks up pending commits by queue position (reorderable via UI); wakeups are server-internal and cross-process workers are coordinated via database state
3. **Only one commit can be evaluated at a time** (enforced by DB unique constraint)
4. Commit marked as `in_progress`
5. `nix-eval-jobs` evaluates all systems in parallel
6. For each system that completes:
   - Policy check runs (is CF enabled for this system?)
   - If **passes**: System derivation → Build Queue
   - If **fails**: System marked as "Policy Failed"
7. When all systems complete: commit marked as `complete`

**Key Database Fields:**
- `commits.evaluation_status`: `pending` | `in_progress` | `complete` | `failed`
- `commits.eval_queue_position`: Order in queue (nullable, user-reorderable)
- Unique constraint: Only one commit can have `evaluation_status = 'in_progress'`

**Startup Behavior:**
- Server resets ALL `in_progress` commits → `pending` on startup
- This prevents orphaned states from crashes/restarts

**Key APIs:**
- `GET /api/v1/commits/eval-queue` - View evaluation queue
- `POST /api/v1/commits/eval-queue/reorder` - Change queue order
- WebSocket: Real-time eval log streaming and system status updates

## Stage 2: Build Queue

After evaluation, derivations enter the build queue:

1. System derivations that **passed policy** are added to build queue
2. Builder picks up jobs from queue
3. Builder runs `nix build`
4. On success: push to cache (if configured)
5. On failure: report error, allow retry

**Key Database Fields:**
- `derivations.status_id`:
  - `3` = dry-run-pending
  - `4` = dry-run-inprogress
  - `5` = dry-run-complete
  - `6` = dry-run-failed
  - `7` = build-pending
  - `8` = build-inprogress
  - `10` = build-complete
  - `12` = build-failed

**Startup Behavior:**
- Server resets derivations with `status_id = 8` → `7` on startup
- This prevents stuck builds from crashes/restarts

**Key point:** The build queue is always being processed. Builders continuously build and push to cache until the queue is empty.

**Key APIs:**
- `GET /build-queue` - View pending builds
- `POST /builders/:id/pause` - Pause builder

## Critical Invariant: Single Active Evaluation

**Why?** nix-eval-jobs is resource-intensive and evaluations should complete before starting new ones.

**How enforced:**
- Unique partial index: `idx_commits_single_in_progress` on `commits(evaluation_status) WHERE evaluation_status = 'in_progress'`
- Attempts to mark a second commit as `in_progress` fail with constraint violation
- Evaluation loop processes pending commits serially

**Status Alignment:**
Both Flakes view and Evaluations view use `commits.evaluation_status` as the single source of truth (not derivation status).

## Related concepts

- [Event-driven queue architecture](../architecture/event-driven-queues.md) - how the queues are woken
- [Derivation status lifecycle](../concepts/derivation-status-lifecycle.md) - status IDs used by the build queue
- [Commit to deploy flow](commit-eval-build-cache-deploy-flow.md) - end-to-end flow chart
- [Commit to deploy sequence](commit-eval-build-cache-deploy-sequence.md) - end-to-end sequence diagram
- [Deployment flow](../deployment/deployment-flow.md) - what happens after the build
