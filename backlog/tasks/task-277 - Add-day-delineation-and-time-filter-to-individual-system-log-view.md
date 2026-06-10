---
id: TASK-277
title: Add day delineation and time filter to individual system log view
status: Backlog
assignee: []
created_date: '2026-04-19 12:30'
updated_date: '2026-06-10 02:59'
labels:
  - ui
  - logs
  - enhancement
  - api
  - backend
  - frontend
milestone: m-19
dependencies: []
priority: medium
ordinal: 2000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The individual system log view currently displays agent events (state changes and heartbeats) as a continuous stream without visual separation between days, making it difficult to navigate and understand temporal patterns. Additionally, there's no way to filter logs by time range.

## Problem
- Log entries from different days blend together without visual separation
- No ability to filter logs by date/time range (API endpoint `/api/v1/systems/:id/agent-events` has no query parameters)
- Currently hard-coded to return last 300 events with no pagination
- Difficult to navigate to specific time periods
- Hard to identify patterns or issues that span multiple days
- Timestamps show full datetime on every entry, which is redundant when grouped by day

## Desired Outcome
- Clear visual delineation between different days in the log view (date headers + dividers)
- Time filter UI (date range picker) positioned above log entries
- Default to last 24 hours of logs
- Pagination within the selected time range
- Improved timestamp display: relative time (e.g., "2 hours ago") with full datetime available
- Backend API support for time-based filtering

## Replan note
This task had a stale open MR (`!241`) during backlog cleanup. It is reset to Backlog for reconciliation under the broader System Detail parity plan.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Day headers (e.g., 'April 19, 2026') appear between log entries from different days
- [ ] #2 Visual divider lines separate entries from different days (headers + dividers)
- [ ] #3 Date range picker UI is positioned above the log entries
- [ ] #4 Default time range is last 24 hours when viewing logs
- [ ] #5 Users can select custom start and end dates/times
- [ ] #6 Pagination works within the selected time range
- [ ] #7 Individual log entry timestamps show relative time with full datetime accessible (tooltip, secondary text, or similar)
- [ ] #8 Backend API accepts optional 'since' and 'before' query parameters (DateTime)
- [ ] #9 Database query filters by timestamp range when parameters are provided
- [ ] #10 Frontend API client constructs URLs with time filter query parameters
- [ ] #11 Empty state message when no logs exist in the selected time range
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Reset to Backlog during cleanup. Treat as System Detail parity sub-work and reconcile stale MR !241 before future execution.
<!-- SECTION:NOTES:END -->
