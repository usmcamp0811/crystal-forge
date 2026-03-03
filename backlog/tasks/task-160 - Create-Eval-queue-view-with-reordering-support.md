---
id: TASK-160
title: Create Eval queue view with reordering support
status: Review
assignee: []
created_date: '2026-03-02 16:22'
updated_date: '2026-03-03 14:25'
labels: []
dependencies: []
priority: medium
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

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
LOCK: OpenCode on reckless in /home/mcamp/code/crystal-forge/TASK-160-eval-queue-view

MR: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/151

Pushed branch: TASK-160-eval-queue-view

Implemented fixes for eval log overflow containment and queue ordering alignment with eval_queue_position.

Verification run: nix develop -c cargo check --manifest-path packages/default/Cargo.toml (pass, warnings only).

Verification run: nix develop -c cargo check --manifest-path packages/web-ui/Cargo.toml (pass, warnings only).
<!-- SECTION:NOTES:END -->
