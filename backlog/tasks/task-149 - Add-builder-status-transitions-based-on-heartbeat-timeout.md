---
id: TASK-149
title: Add builder status transitions based on heartbeat timeout
status: Backlog
assignee: []
created_date: '2026-03-01 03:25'
labels:
  - security
  - reliability
  - builder-api
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Builders should automatically transition through states based on their last heartbeat time to reflect their actual availability.

Current behavior:
- Builder status becomes "active" on first heartbeat
- Status remains "active" indefinitely even if heartbeats stop

Desired behavior:
- Active: Last heartbeat within 30 minutes
- Warning/Stale (name TBD): Last heartbeat between 30-60 minutes ago
- Offline: Last heartbeat over 60 minutes ago (or never received)

This ensures the system has an accurate view of which builders are actually available for work assignment.

Implementation considerations:
- May need to add new status variant to BuilderStatus enum (e.g., "Stale", "Warning", "Degraded")
- Need periodic check to update builder statuses (background task, cron-like job, or check-on-demand)
- Should not assign jobs to builders in warning/offline state
- Consider whether to keep running jobs when builder transitions to warning state
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 BuilderStatus enum includes new state for 30-60min timeout (warning/stale/degraded)
- [ ] #2 Builders with last_heartbeat_at older than 30 minutes transition to warning state
- [ ] #3 Builders with last_heartbeat_at older than 60 minutes transition to offline state
- [ ] #4 Builders with NULL last_heartbeat_at are considered offline
- [ ] #5 Status transitions happen automatically (background job or on-demand check)
- [ ] #6 Job assignment logic excludes builders in warning or offline state
- [ ] #7 Existing tests updated to account for new status transitions
- [ ] #8 Migration adds new status variant if using database enum
<!-- AC:END -->

## Definition of Done
<!-- DOD:BEGIN -->
- [ ] #1 Database migration created if BuilderStatus is stored as enum
- [ ] #2 Background task or check mechanism implemented and tested
- [ ] #3 Documentation updated with builder status lifecycle
<!-- DOD:END -->
