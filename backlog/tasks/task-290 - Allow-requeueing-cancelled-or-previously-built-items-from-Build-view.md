---
id: TASK-290
title: Allow requeueing cancelled or previously built items from Build view
status: Backlog
assignee: []
created_date: '2026-05-08 02:20'
labels:
  - feature
  - builds
  - ui
  - api
  - requeue
milestone: Build Queue UX
dependencies: []
priority: high
ordinal: 2900
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Enable operators to requeue build jobs from the Build view for entries that are currently cancelled or already completed, so they can be rebuilt on demand.

## Problem
In the current Build view workflow, users cannot reliably trigger a rebuild for jobs that were cancelled or already built. This blocks common recovery and re-validation flows (e.g., rerun after transient infra issues, cache issues, or policy changes).

## Desired Outcome
From the Build view, users can select eligible jobs (cancelled and completed) and explicitly requeue them so the system creates a new build execution for the same derivation/commit target.

## Scope Notes
- This task should cover Build view UX and backend/API behavior needed to initiate a rebuild.
- Eligibility rules and guardrails should be explicit (e.g., which statuses are requeueable).
- Requeue actions should be auditable and reflected in queue/history state.

## Initial Acceptance Direction
- Requeue action visible for cancelled/completed entries in Build view.
- Triggering requeue creates a new queued build attempt.
- UI updates clearly show the new queued/running attempt.
- Existing queue ordering and cancellation flows remain intact.

This is backlog capture and will need sprint-ready refinement before implementation (non-goals, architecture constraints, verification plan, and objective AC details).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Build view shows a requeue action for eligible cancelled/completed build entries
- [ ] #2 Requeue action creates a new build attempt in queued state for the same target
- [ ] #3 User receives clear success/failure feedback when requeue is requested
- [ ] #4 New attempt appears in queue/history without corrupting existing attempt records
- [ ] #5 Unauthorized users cannot requeue builds
- [ ] #6 Existing cancel/reorder queue behavior is unaffected
<!-- AC:END -->
