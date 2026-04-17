---
id: TASK-247
title: Add heartbeat countdown timer to system detail view
status: Backlog
assignee: []
created_date: '2026-04-05 22:33'
labels:
  - frontend
  - ux
  - monitoring
  - systems
  - agents
  - observability
dependencies: []
references:
  - packages/web-ui/src/views/system_detail.rs
  - packages/default/src/handlers/api/systems.rs
  - packages/default/src/queries/systems.rs
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

Operators and users have no visibility into when the next agent heartbeat is expected for a system. This creates uncertainty during deployment workflows and troubleshooting:

- When a new configuration is ready to deploy, users cannot estimate when the agent will discover and apply it
- During incident response, operators cannot tell if an agent is "about to check in" vs "overdue" vs "dead"
- No visual feedback for heartbeat cadence or agent polling behavior
- Users waiting for deployments have no sense of timing - could be seconds or minutes
- Distinguishing between "agent hasn't polled yet" vs "agent is offline" requires manual timestamp math

This information is critical for:
- Deployment timing expectations ("how long until this applies?")
- Agent health monitoring and alerting
- Operator decision-making (wait vs investigate vs manual intervention)
- Understanding system responsiveness

## Goal

Add a live countdown timer to the system detail view showing time remaining until the next expected agent heartbeat, so users can see at-a-glance when the agent is expected to check in and pick up pending work.

## Non-Goals

- This task does NOT change agent heartbeat intervals or polling behavior
- This task does NOT implement agent health alerting or notifications
- This task does NOT add the ability to manually trigger agent check-ins
- This task does NOT modify agent-side code or heartbeat logic
- This task does NOT implement historical heartbeat analytics or graphs
- This task does NOT add heartbeat configuration UI

## Scope

1. Calculate expected next heartbeat time based on last heartbeat timestamp and known/configured heartbeat interval
2. Add live countdown timer component to system detail view showing time until next expected heartbeat
3. Provide clear visual states: healthy (green countdown), overdue (red/warning with time since last), never seen (gray "waiting for first heartbeat")
4. Timer updates in real-time (ticks down every second)
5. Show both countdown ("next heartbeat in 45s") and absolute timestamp ("expected at 14:23:15")
6. Handle edge cases: agent never connected, heartbeat overdue, stale data

## Architectural Constraints

- Countdown calculation MUST happen client-side to avoid constant server polling
- Backend MUST provide last heartbeat timestamp and heartbeat interval in system detail API response
- UI timer MUST update smoothly without causing layout shifts or flicker
- Visual states MUST be immediately understandable (color-coded, clear labels)
- Component MUST gracefully handle missing heartbeat data (new agents, never-connected systems)
- Timer MUST account for clock skew between client and server (use server-provided timestamps as authority)
- MUST NOT introduce performance issues with continuous re-renders
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 System detail view displays live countdown timer showing seconds until next expected agent heartbeat
- [ ] #2 Timer updates every second with smooth animation and no layout flicker
- [ ] #3 Visual states clearly distinguish: healthy (green countdown), overdue (red warning), never seen (gray waiting)
- [ ] #4 Display shows both countdown (relative time) and absolute timestamp for next expected heartbeat
- [ ] #5 Timer handles edge cases gracefully: no heartbeat data, overdue agent, stale information
- [ ] #6 Backend API provides necessary data: last heartbeat timestamp and heartbeat interval for the system
- [ ] #7 Countdown calculation happens client-side without requiring continuous API polling
<!-- AC:END -->
