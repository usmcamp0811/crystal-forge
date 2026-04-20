---
id: TASK-282
title: Fix builder stuck in "building" state after server restart
status: Backlog
assignee: []
created_date: '2026-04-20 19:22'
labels:
  - bug
  - builder
  - infrastructure
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

When the server restarts (and possibly other scenarios), builds that are in progress become stuck in the "building" state. The builder process stops building, but the server-side state is not updated to reflect this. This clogs the build queue and prevents new builds from starting.

**Observable symptoms:**
- Build shows "building" status for over 1 hour
- The commit has already been successfully built for other systems (config should be cached)
- Builds that should complete quickly (due to cache) remain stuck indefinitely
- Build queue becomes blocked

**Root cause hypothesis:**
Server restart kills the builder process, but the server's build state tracking is not cleaned up or synchronized on restart.

## Desired Outcome

Builds cannot become permanently stuck in "building" state. The server correctly detects and recovers from orphaned builds.

## Proposed Solution (needs validation)

On server restart, reset any builds that are in "building" state to a recoverable state (e.g., pending/queued, or failed with a specific restart marker).

## Open Questions

1. **State recovery behavior:** When the server restarts and finds builds in "building" state, what should happen to them?
   - Option A: Reset to pending/queued (retry automatically)
   - Option B: Mark as failed with a "server-restart" reason
   - Option C: Mark as cancelled/aborted
   - Option D: Other?

2. **Builder communication model:** How does the server currently communicate with the builder process?
   - Is it a separate process?
   - Is there a health check or heartbeat mechanism?
   - Could we detect builder death during normal operation (not just on restart)?

3. **Build queue design:** 
   - Is there a maximum number of concurrent builds?
   - Does one stuck build block all builds, or only builds for that system/configuration?
   - Are builds processed FIFO, or is there prioritization?

4. **Scope boundaries:**
   - Should we ONLY handle server restart, or also detect builder process crashes during normal operation?
   - Should we implement builder heartbeat/keepalive, or just cleanup on restart?
   - Should we add observability (logs, metrics, alerts) for stuck builds?

5. **Manual recovery:** 
   - Is there currently a manual way to unstick a build (admin endpoint, CLI command)?
   - Should we add one as part of this task?

6. **Persistence layer:**
   - Where is build state stored (database, in-memory, both)?
   - Is there a timestamp for when a build started?
   - Could we use a build timeout as a safeguard (mark builds as failed if building > X minutes)?

## Impact Areas

- Server startup/initialization logic
- Build queue management
- Builder process lifecycle
- Build state tracking
- Possibly: builder health monitoring
- Possibly: admin tooling for manual intervention
<!-- SECTION:DESCRIPTION:END -->
