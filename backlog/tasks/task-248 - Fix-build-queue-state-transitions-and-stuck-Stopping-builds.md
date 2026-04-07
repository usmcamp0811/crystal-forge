---
id: TASK-248
title: Fix build queue state transitions and stuck "Stopping" builds
status: Backlog
assignee: []
created_date: '2026-04-07 23:27'
labels:
  - bug
  - build-queue
  - state-management
  - ui
  - backend
dependencies: []
references:
  - Build queue UI components
  - Build state machine implementation
  - Database migrations for build status enum
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Currently, builds that are stopped get stuck in a "Stopping" status with no way to clear or restart them. The build queue needs proper state management to allow builds to transition between states and provide recovery mechanisms for stuck builds.

## Problem
- Clicking "Stop" on a build leaves it in "Stopping" status indefinitely
- No way to clear or restart stopped/stuck builds
- Missing state transitions between queue states
- Users cannot recover from interrupted builds

## Current State Flow Issues
- Queue → Building → (stuck in "Stopping")
- No path to Cancelled/Stopped final states
- No restart/retry mechanism

## Desired State Flow
- Queue → Building → Cancelled/Stopped (terminal states)
- Queue → Building → Completed (terminal states)
- Ability to restart from Cancelled/Stopped → Queue
- Ability to force-cancel stuck builds
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Builds can successfully transition from 'Building' to 'Stopped' or 'Cancelled' terminal state
- [ ] #2 Builds in 'Stopping' status can be force-cancelled to reach terminal state
- [ ] #3 Users can restart a stopped/cancelled build, moving it back to 'Queue' status
- [ ] #4 UI provides clear actions for each build state (Stop, Restart, Clear, etc.)
- [ ] #5 Database schema supports all required build states and transitions
- [ ] #6 Build state transitions are properly validated and logged
- [ ] #7 Orphaned/stuck builds can be identified and recovered (manual or automatic cleanup)
<!-- AC:END -->
