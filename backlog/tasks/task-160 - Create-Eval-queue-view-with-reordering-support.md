---
id: TASK-160
title: Create Eval queue view with reordering support
status: In Progress
assignee: []
created_date: '2026-03-02 16:22'
updated_date: '2026-03-07 03:16'
labels: []
dependencies: []
priority: medium
ordinal: 1000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Create a dedicated Eval view that mirrors the Build view functionality.

## Problem

Currently, evaluation logs are only accessible via a chip from the Flake view. There is no centralized place to:
- See all pending/running evaluations
- Monitor evaluation progress across multiple flakes/commits
- Reorder evaluation priority
- See policy check results per system in real-time

## Flow

1. Commit enters eval queue
2. nix-eval-jobs evaluates each system in parallel
3. For each system that completes eval → run policy check (CF enabled?)
4. If policy passes → system gets added to build queue
5. If policy fails → mark as failed/skipped

## Desired Outcome

Create a new "Evals" view (similar to "Builds" view) that:
- Shows all queued and active evaluations
- Displays each commit (not individual systems - since nix-eval-jobs does parallel evaluation)
- Shows system count and status per commit
- Real-time per-system status updates (chips on Flake view should also update)
- Policy check status indicator per system (passed/failed)
- Supports drag-and-drop or up/down arrows for reordering
- Queue order persists to database
- The Flake view eval log chip redirects to the Evals view

## Impact Areas

- New Evals view/page in web-ui
- Evaluation queue management (backend + frontend)
- Move eval log functionality from Flake view to Evals view
- Navigation updates (sidebar, routing)
- Backend API for queue ordering and real-time status
- WebSocket updates for per-system status during eval
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 New Evals view accessible via sidebar navigation
- [ ] #2 Eval queue displays commits in order (similar to Build queue)
- [ ] #3 Each commit row shows: flake name, branch, commit hash, system count, overall status
- [ ] #4 Per-commit progress indicator showing passed/total (e.g., '3/5 passed')
- [ ] #5 Queue supports reordering via up/down arrows and drag-and-drop
- [ ] #6 Queue order persists to database
- [ ] #7 Real-time updates via WebSocket as systems are evaluated
- [ ] #8 Per-system status chips update in real-time during eval (pending → evaluating → success/failed)
- [ ] #9 Policy check status shown per system: passed policy → added to build queue (green indicator)
- [ ] #10 Policy failed systems shown with 'Policy Failed' status (yellow/orange) with tooltip explaining what that means
- [ ] #11 Systems that pass policy are removed from eval queue after a short delay (2-3 seconds) to show queue progression
- [ ] #12 Flake view eval log chip redirects to Evals view instead of opening modal
- [ ] #13 Completed evals shown with final status in queue
- [ ] #14 Status items have explanatory text or hover tooltips explaining what each status means
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
## Event-Driven Queue Architecture Implementation

### Current State (Polling)
- Eval loop: polls every 60s for pending commits
- Build workers: poll every 5s when idle
- High latency (up to 60s for eval, 5s for builds)
- Wastes CPU cycles polling empty queues

### Target State (Event-Driven)
- Both queues wake immediately when work arrives
- FIFO ordering maintained via MPSC channels
- Idle when empty (no polling)
- Fallback periodic tick to catch missed events

### Implementation Steps

1. **Create QueueNotification System** (`src/queue/mod.rs`)
   - MPSC unbounded channels for eval and build queues
   - Shared state in server initialization
   - Thread-safe Arc wrapper

2. **Modify Eval Loop** (`src/server/mod.rs`)
   - Replace pure polling with `tokio::select!`
   - Listen to eval channel + 60s fallback ticker
   - Process commits immediately on notification

3. **Modify Build Workers** (`src/builder/worker.rs`)
   - Replace 5s sleep with notification listener
   - Listen to build channel + 5s fallback ticker
   - Claim work immediately on notification

4. **Add Notification Triggers**
   - `insert_commit_with_metadata`: Notify eval queue
   - `create_build_jobs_for_commit`: Notify build queue
   - Handle flake polling triggers
   - Handle webhook triggers (future)

5. **Update Architecture Docs** (`docs/architecture.md`)
   - Document event-driven queue design
   - Explain FIFO guarantees
   - Document fallback polling behavior

### Safety Considerations
- Keep fallback polling to catch missed notifications
- MPSC channels are unbounded (notifications are tiny)
- No blocking operations in notification path
- Notifications are fire-and-forget (no errors on dropped receiver)

### Testing
- Verify immediate eval on commit insert
- Verify immediate build on job creation
- Verify no busy-wait when queues are empty
- Verify FIFO ordering is maintained
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
LOCK: OpenCode on reckless in /home/mcamp/code/crystal-forge/TASK-160-eval-queue-view

MR: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/151

Pushed branch: TASK-160-eval-queue-view

Implemented fixes for eval log overflow containment and queue ordering alignment with eval_queue_position.

Verification run: nix develop -c cargo check --manifest-path packages/default/Cargo.toml (pass, warnings only).

Verification run: nix develop -c cargo check --manifest-path packages/web-ui/Cargo.toml (pass, warnings only).

## Queue Reset Enforcement Complete (2026-03-02)

### Implemented
- ✅ Migration 0088: Enforces single active evaluation at DB level with unique partial index
- ✅ `reset_stuck_commit_evaluations`: Now resets ALL in_progress commits on startup (not just >30min)
- ✅ `reset_stuck_builds`: New function resets derivations with status_id=8 (build-inprogress) → 7 (build-pending)
- ✅ `mark_commit_evaluation_started`: Returns clear error when another commit is already in_progress (constraint violation)
- ✅ Server startup sequence calls both reset functions before eval loop starts

### Testing
- ✅ Verified unique constraint prevents multiple in_progress commits
- ✅ Backend compiles successfully
- ✅ Frontend compiles successfully
- ✅ SQLx metadata regenerated

### Commit
- `63804056` - feat: enforce single active evaluation and reset queues on startup

This fixes the issue where multiple commits could be marked in_progress simultaneously, ensuring clean queue state on server restart.

## Eval Status Alignment Fix (2026-03-02)

### Problem
Flakes view and Evaluations view were showing different statuses for the same commits:
- Flakes view: Used derivation dry-run status (status_id 3,4,5,6) showing 'queued'
- Evaluations view: Used commits.evaluation_status showing 'complete'

### Solution
- Updated Flakes timeline query to use `commits.evaluation_status` directly
- Removed complex derivation status subquery
- Updated frontend badge labels to match commit status values:
  - `pending` → displays as 'queued'
  - `in_progress` → displays as 'running'
  - `complete` → displays as 'complete'
  - `failed` → displays as 'failed'

### Result
✅ Both views now use the same source of truth
✅ Status chips align correctly between Flakes and Evaluations views

### Commit
- `36958f51` - fix: align Flakes view eval status with Evaluations view

Resuming post-review fixes for MR 151 based on merge-blocker checklist.
LOCK: OpenCode on reckless in /home/mcamp/code/crystal-forge/TASK-160-eval-queue-view

Post-review MR-151 hardening pass completed:
- Reorder validation now reports explicit duplicate/missing/extra ID sets and returns structured 400 response body.
- Reorder + insert paths now use a shared advisory transaction lock for queue-position consistency under concurrency.
- Active queue ordering tie-breakers now include immutable id fallback after eval_queue_position + commit_timestamp.
- Added regression tests for reorder validation paths and queue coalescing follow-up wake behavior.
- Eval websocket reconnect now explicitly tears down prior socket before opening a new stream.
- Updated architecture doc to match bounded channel(1) coalescing and process-before-wait loop ordering.

Verification executed:
- nix develop -c env SQLX_OFFLINE=true cargo test --manifest-path packages/default/Cargo.toml queries::commits::tests -- --nocapture (pass)
- nix develop -c env SQLX_OFFLINE=true cargo test --manifest-path packages/default/Cargo.toml queue::tests -- --nocapture (pass)
- nix develop -c cargo check --manifest-path packages/web-ui/Cargo.toml (pass)

Note: nix develop -c cargo check --manifest-path packages/default/Cargo.toml without SQLX_OFFLINE requires a running DB in this repo and failed in this session due connection refused.

Committed and pushed MR-151 hardening updates.
Commit: ee17bffd
Push: TASK-160-eval-queue-view -> origin (GitLab HTTPS push using glab token auth).
<!-- SECTION:NOTES:END -->
