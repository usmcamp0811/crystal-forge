---
id: TASK-248
title: Fix build queue state transitions and stuck "Stopping" builds
status: Review
assignee:
  - agent-claude
created_date: '2026-04-07 23:27'
updated_date: '2026-04-08 02:51'
labels:
  - bug
  - build-queue
  - state-management
  - ui
  - backend
milestone: Build Queue Reliability
dependencies: []
references:
  - Build queue UI components
  - Build state machine implementation
  - Database migrations for build status enum
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/216'
priority: high
ordinal: 248000
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
- [x] #1 Builds can successfully transition from 'Building' to 'Stopped' or 'Cancelled' terminal state
- [x] #2 Builds in 'Stopping' status can be force-cancelled to reach terminal state
- [x] #3 Users can restart a stopped/cancelled build, moving it back to 'Queue' status
- [x] #4 UI provides clear actions for each build state (Stop, Restart, Clear, etc.)
- [x] #5 Database schema supports all required build states and transitions
- [x] #6 Build state transitions are properly validated and logged
- [x] #7 Orphaned/stuck builds can be identified and recovered (manual or automatic cleanup)
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
## Implementation Plan

1. **Database Schema Migration**
   - Create migration to update build_status enum
   - Add stopped_at timestamp column
   - Add can_restart boolean (default true for terminal states)
   - Test migration up/down

2. **Backend State Machine**
   - Define BuildStatus enum with all states
   - Implement state transition validation logic
   - Add transition methods: stop(), cancel(), restart()
   - Write unit tests for state transitions

3. **API Endpoints**
   - Implement POST /api/builds/{id}/stop
   - Implement POST /api/builds/{id}/cancel
   - Implement POST /api/builds/{id}/restart
   - Add authorization checks
   - Write integration tests

4. **Build Worker Updates**
   - Add graceful shutdown handler for "Stopping" state
   - Implement transition to "Stopped" on clean shutdown
   - Add timeout logic (30s) for Stopping → Cancelled
   - Test worker behavior with mock builds

5. **UI Components**
   - Update build queue list to show state-specific buttons
   - Add "Stop" button (Building state only)
   - Add "Force Cancel" button (Stopping state)
   - Add "Restart" button (Stopped/Cancelled/Failed states)
   - Update build status badge styling for new states

6. **Cleanup Job**
   - Implement background task to scan for stuck builds
   - Auto-cancel builds in "Stopping" > timeout threshold
   - Add admin notification for repeated stuck builds
   - Schedule job (e.g., every 5 minutes)

7. **Testing & Verification**
   - Manual test: stop build → verify reaches "Stopped"
   - Manual test: force cancel stuck build → verify "Cancelled"
   - Manual test: restart stopped build → verify re-queued
   - Run full test suite
   - Verify database migrations apply cleanly
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Follow-up alignment: backend force-cancel narrowed from (building|cancelling) to cancelling-only to match UI behavior and avoid semantic mismatch. CAS guard retained, so terminal states cannot be clobbered.
<!-- SECTION:NOTES:END -->

## Definition of Done
<!-- DOD:BEGIN -->
- [ ] #1 All database migrations applied successfully in dev environment
- [ ] #2 Unit tests written and passing for state machine logic
- [ ] #3 Integration tests written and passing for API endpoints
- [ ] #4 Worker graceful shutdown tested with real build process
- [x] #5 UI components manually tested for all state transitions
- [x] #6 Code passes cargo fmt and cargo clippy checks
- [ ] #7 SQLX metadata synced (cargo sqlx prepare)
- [x] #8 Documentation updated for new build states and transitions
<!-- DOD:END -->
