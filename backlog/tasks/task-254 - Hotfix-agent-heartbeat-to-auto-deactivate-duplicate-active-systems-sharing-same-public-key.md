---
id: TASK-254
title: >-
  Hotfix agent heartbeat to auto-deactivate duplicate active systems sharing
  same public key
status: To Do
assignee: []
created_date: '2026-04-09 16:31'
labels:
  - hotfix
  - agent
  - systems
  - health
milestone: m-12
dependencies: []
references:
  - packages/default/src/handlers/agent/heartbeat.rs
  - packages/default/src/queries/systems.rs
priority: high
ordinal: 2600
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Problem:
When a host is re-joined under a new hostname (e.g. `base` -> `nix-builder`) while reusing the same agent keypair, both system records can remain active in `systems`. The stale hostname then appears critical/offline in health views, causing confusing fleet/system health despite current heartbeats from the renamed host.

Desired outcome:
On successful agent heartbeat authentication, if other active system records share the exact same public key but a different hostname, automatically deactivate those duplicate records and log the action. This keeps only the current hostname active for a given keypair and prevents stale duplicate entries from skewing health.

Scope:
- Backend-only hotfix
- No schema migration
- No UI changes
- Preserve existing auth semantics (hostname + signature)
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 When an authenticated heartbeat arrives for hostname H with public key K, any other active system rows with public key K and hostname != H are set inactive
- [ ] #2 Current hostname H remains active and heartbeat processing continues normally
- [ ] #3 Server logs include which duplicate hostnames were auto-deactivated
- [ ] #4 No behavior change for non-duplicate cases
<!-- AC:END -->
