---
id: TASK-277
title: Add day delineation and time filter to individual system log view
status: Backlog
assignee: []
created_date: '2026-04-19 12:30'
updated_date: '2026-04-19 12:39'
labels:
  - ui
  - logs
  - enhancement
  - api
  - backend
  - frontend
dependencies: []
priority: medium
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

## Context
- Current endpoint: `GET /api/v1/systems/:id/agent-events`
- Handler: `packages/default/src/handlers/api/systems.rs:get_system_agent_events`
- Query function: `packages/default/src/queries/systems.rs:list_system_agent_event_rows`
- Frontend component: `packages/web-ui/src/components/system/tabs/logs_tab.rs`
- API client: `packages/web-ui/src/api/client.rs:fetch_system_agent_events`
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Day headers (e.g., 'April 19, 2026') appear between log entries from different days
- [ ] #2 Visual divider lines separate entries from different days (headers + dividers)
- [ ] #3 Date range picker UI is positioned above the log entries
- [ ] #4 Default time range is last 24 hours when viewing logs
- [ ] #5 Users can select custom start and end dates/times
- [ ] #6 Pagination works within the selected time range
- [ ] #7 Individual log entry timestamps show relative time (e.g., '2 hours ago') with full datetime accessible (tooltip, secondary text, or similar)
- [ ] #8 Backend API accepts optional 'since' and 'before' query parameters (DateTime)
- [ ] #9 Database query filters by timestamp range when parameters are provided
- [ ] #10 Frontend API client constructs URLs with time filter query parameters
- [ ] #11 Empty state message when no logs exist in the selected time range
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Add `SystemAgentEventsParams` struct to API models with optional datetime fields
2. Update handler to extract query parameters and pass to database function
3. Modify database query to support optional time filtering with WHERE clause
4. Update frontend API client to construct URLs with query parameters
5. Add date range picker component above logs in LogsTab
6. Implement day grouping logic in frontend (group events by date)
7. Add date headers and divider rendering between day groups
8. Update timestamp display to show relative time with full datetime
9. Set default time range to last 24 hours
10. Add empty state handling for no logs in selected range
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
## Architecture Changes Required

### Backend (Rust)
1. **API Models** (`packages/default/src/api/models.rs`)
   - Create `SystemAgentEventsParams` struct with optional `since: Option<DateTime<Utc>>` and `before: Option<DateTime<Utc>>`
   - Follow pattern from existing `SystemsListParams` (lines 760-769)

2. **Handler** (`packages/default/src/handlers/api/systems.rs`)
   - Update `get_system_agent_events` signature to extract `Query<SystemAgentEventsParams>`
   - Pass time parameters to database query function

3. **Database Query** (`packages/default/src/queries/systems.rs`)
   - Update `list_system_agent_event_rows` to accept optional time bounds
   - Modify SQL query to include `WHERE timestamp >= $x AND timestamp <= $y` when parameters are provided
   - Consider pagination parameters (offset/limit) for future extensibility

### Frontend (Dioxus)
1. **LogsTab Component** (`packages/web-ui/src/components/system/tabs/logs_tab.rs`)
   - Add date range picker UI above log list
   - Implement day grouping logic (group events by date)
   - Render date headers and dividers between days
   - Update timestamp display to show relative time with full datetime
   - Add state for selected time range (default: last 24h)
   - Trigger API refetch when time range changes

2. **API Client** (`packages/web-ui/src/api/client.rs`)
   - Update `fetch_system_agent_events` to accept optional `since` and `before` parameters
   - Construct query string with time parameters

## Implementation Order
1. Backend API changes (models, handler, query)
2. Frontend API client update
3. Frontend UI component (filter controls)
4. Frontend day grouping and display logic
5. Timestamp formatting improvement

## Architectural Constraints
- Backend API must remain backward compatible (query params are optional)
- Follow existing patterns: SystemsListParams for query params, similar WHERE clause patterns
- UI layer must not contain business logic (filtering happens in backend)
- Database query must use parameterized queries (no SQL injection risk)
- Date/time handling must use chrono::DateTime<Utc> consistently
- Frontend must handle timezone conversion for display
- No breaking changes to existing API response structure

## Verification Plan
**Tier:** Tier 1 (Feature-level integration)

**Commands:**
1. Backend unit tests:
   - nix develop -c cargo test queries::systems::list_system_agent_event_rows
   - nix develop -c cargo test handlers::api::systems::get_system_agent_events
   
2. Integration check:
   - nix develop -c server-stack up
   - Navigate to system detail page, logs tab
   - Verify default shows last 24h
   - Select custom date range, verify logs update
   - Verify day headers appear between different days
   - Verify timestamps show relative time
   - Verify empty state when no logs in range

3. Full verification:
   - nix develop -c cargo fmt -- --check
   - nix develop -c cargo clippy -- -D warnings
   - nix develop -c cargo test
   - nix flake check (includes sqlx metadata sync)

**Screenshot Required:** UI showing date headers, dividers, and time filter controls

## Impact Areas
- Backend: API models, handlers, queries
- Frontend: LogsTab component, API client
- Database: Query performance (new WHERE clauses, potential index consideration)
- User experience: Default behavior changes (24h limit vs 300 events)

## Risk Level
**Medium**
- Database query changes could impact performance on large log tables
- Frontend grouping logic adds complexity
- Default time range changes user-visible behavior

## Dependencies
None - this is a standalone enhancement
<!-- SECTION:NOTES:END -->
