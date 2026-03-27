---
id: TASK-201
title: Dashboard Fleet Health widget showing mock data instead of real systems
status: To Do
assignee: []
created_date: '2026-03-20 13:40'
updated_date: '2026-03-20 13:40'
labels:
  - frontend
  - dashboard
  - bug
  - high-priority
dependencies: []
references:
  - packages/web-ui/src/views/dashboard.rs
  - packages/web-ui/src/components/dashboard/fleet_health.rs
  - packages/default/src/queries/dashboard.rs
priority: high
ordinal: 1200
---

# Dashboard Fleet Health widget showing mock data instead of real systems

---

# Problem Statement

The Fleet Health widget on the Dashboard view displays hardcoded mock data (e.g., "server-01") instead of actual system health data from the database. This makes the dashboard misleading and prevents users from seeing real fleet status.

---

# Goal

Fleet Health widget displays real system data from the database, including actual system hostnames, health status, agent connectivity, and build/deployment status.

---

# Non-Goals

- Redesigning the Fleet Health widget UI
- Adding new health metrics or checks
- Implementing WebSocket real-time updates (use existing pattern)
- Changing dashboard layout or widget sizing
- Adding filtering by environment (separate task)

---

# Acceptance Criteria

- [ ] Fleet Health widget queries real data via `/api/dashboard` endpoint
- [ ] Widget displays actual system hostnames from database
- [ ] Widget shows accurate health status for each system:
  - Healthy (agent connected, recent successful deployment)
  - Warning (agent connected, deployment issues)
  - Critical (agent disconnected or system errors)
- [ ] Widget shows agent connectivity status (connected/disconnected)
- [ ] Empty state shown gracefully when no systems registered
- [ ] Mock data removed from frontend code
- [ ] Backend query uses real `systems` table data
- [ ] Data fetching follows existing dashboard pattern (spawn + async fetch)
- [ ] Loading state shown while fetching data
- [ ] Error state shown on fetch failure

---

# Architectural Constraints

- Follow existing dashboard data fetching pattern (see dashboard.rs line 159, 182)
- Use existing `/api/dashboard` endpoint or extend it
- Backend queries in `queries/dashboard.rs` module
- No hardcoded mock data in production code
- UI components in `components/dashboard/` directory
- Use existing health status types/enums
- No schema changes (use existing systems table)

---

# Verification Plan

Automated:
- `nix develop -c cargo test queries::dashboard`
- `nix develop -c cargo clippy -- -D warnings`
- `nix develop -c cargo fmt -- --check`
- UI build: `nix build .#web-ui`

Manual:
- Start dev stack with registered systems
- Navigate to Dashboard
- Verify Fleet Health widget shows real system names
- Verify health status indicators match actual system state
- Register a new system
  - Refresh dashboard
  - Verify new system appears in Fleet Health widget
- Disconnect an agent
  - Verify widget shows disconnected status
- Test with zero systems registered
  - Verify empty state message shown
- Check browser console for errors

---

# Impact Areas

UI | API | Domain

- Dashboard view (frontend)
- Fleet Health widget component
- `/api/dashboard` endpoint
- `queries/dashboard::fetch_fleet_health`
- System health calculation logic

---

# Risk Level

Low

This is primarily a bug fix replacing mock data with real data. Existing endpoint and query structure should support this. Risk is limited to:
- Query performance if many systems (mitigate with LIMIT/pagination)
- UI breaking if data shape doesn't match expectations

Mitigations:
- Use existing proven dashboard data fetching pattern
- Add proper error handling and empty states
- Test with various system counts (0, 1, 10)

---

# Dependencies

None

---

# Follow-Up Tasks

- Add pagination/grouping for fleet health if >20 systems
- Add filtering by environment to Fleet Health widget
- Add drill-down to system detail from Fleet Health widget
- Add configurable health status thresholds
