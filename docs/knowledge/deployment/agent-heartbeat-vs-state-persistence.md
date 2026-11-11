---
type: Workflow
title: "Agent heartbeat, state, deployment, and history logic"
description: "Entry point for how agents report state, how the server chooses between an agent_heartbeats row and a full system_states row, and the equivalence check; open it to understand heartbeat versus full state persistence."
tags:
  - crystal-forge
  - agent
  - heartbeat
  - system-state
  - persistence
implementation_status: partial
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-03T22:55:03-05:00
sources:
  - id: origin
    resource: "Crystal Forge repository file docs/agent-heartbeat-state-history.md at commit 3b23d36f"
    title: "Agent heartbeat, state, deployment, and history logic"
---
# Agent heartbeat, state, deployment, and history logic

This document describes how Crystal Forge agents report state, how the server
decides whether to persist a lightweight heartbeat or a full system-state row,
how deployment commands are returned, and how event-backed history entries are
classified for the UI.

TASK-508 adds a DB-owned enrollment epoch and transactional ingress checks.
The owner reports passing isolated enrollment/race tests. Full exact-source
integration proof remains pending; see
[management modes](../concepts/management-modes-and-configuration-assurance.md).

`system_events` is the authoritative source for user-facing Deployment History.
`system_states` remains available as raw observation/audit/debug data and as a
legacy fallback for systems that do not yet have event rows.

It is intentionally focused on the bugs seen during TASK-378:

- manual `nixos-rebuild switch` rows showing as Crystal Forge deploys
- the latest generation being hidden behind `Agent restarted`
- unchanged periodic heartbeats filling history with fake `Local rebuild` rows
- compatibility with older agents that send `state_delta` periodically

## Source files

- Agent loop and POST handling: `packages/default/crates/cf-agent/src/bin/agent.rs`
- Agent deployment response handling: `packages/default/crates/cf-agent/src/deployment/agent.rs`
- Server heartbeat route: `packages/default/crates/cf-server/src/handlers/agent/heartbeat.rs`
- Heartbeat-vs-state equivalence logic: `packages/default/crates/cf-server/src/models/agent_heartbeats.rs`
- System state insert/query logic: `packages/default/crates/cf-server/src/queries/system_states.rs`
- System history API classification: `packages/default/crates/cf-server/src/handlers/api/systems.rs`
- Web UI history rendering: `packages/web-ui/src/views/system_detail.rs`

## High-level flow

```mermaid
sequenceDiagram
    participant Agent
    participant Server as Crystal Forge server
    participant DB as PostgreSQL
    participant UI as Web UI

    Agent->>Agent: Gather /run/current-system, generation, boot_id, metadata
    Agent->>Server: POST /agent/heartbeat (SystemState + change_reason)
    Server->>Server: Authenticate agent request
    Server->>DB: Acquire writer, POA&M and system locks and recheck key, epoch, mode, activity
    alt enrollment identity revoked
        Server-->>Agent: 403 and no observation write
    else identity current
        Server->>Server: Determine whether enrollment lacks a current full state
    Server->>Server: Lock previous observed state and classify boot_id change
    Server->>DB: INSERT idempotent system_events for real transitions only
    Server->>Server: Decide heartbeat vs full state row
    alt unchanged heartbeat-equivalent state and enrollment already observed
        Server->>DB: INSERT agent_heartbeats
    else reboot, real state transition or first enrollment observation
        Server->>DB: INSERT system_states
    end
    Server->>DB: Query desired_target and runtime cache config
    Server-->>Agent: LogResponse { desired_target, runtime_caches, heartbeat_interval_secs }
    alt desired_target present and differs from current system
        Agent->>Agent: Copy store path from cache and activate via systemd-run
        Agent->>Server: POST /agent/state with cf_deployment/config_change when reported
    else no desired target or already on target
        Agent->>Agent: No deployment needed
    end
    end
    UI->>Server: GET system history
    Server->>DB: Read system_events history, fallback to system_states if empty
    Server-->>UI: SystemHistoryEntry[] with explicit event_type/event_kind
    UI->>UI: Render deployment timeline from explicit event_type
```

## Server heartbeat-vs-state decision

The heartbeat endpoint chooses a full observation or lightweight telemetry
after transactional authentication. The legacy `/agent/state` endpoint inserts
a full row after the same enrollment guard; it does not classify boot ID.
Both write paths hold the system-row lock through observation commit. A stale
key or epoch, nonmanaged mode, or inactive registration denies the write.

```mermaid
flowchart TD
    A[POST /agent/heartbeat or /agent/state] --> B[Authenticate request]
    B --> C[Deserialize SystemState]
    C --> Guard{Current key, epoch, managed mode and activity under lock?}
    Guard -->|no| Denied[Reject write]
    Guard -->|yes| Route{Heartbeat endpoint?}
    Route -->|no| I[Insert full system_states row]
    Route -->|yes| Epoch{Enrollment has a current full observation?}
    Epoch -->|no| New[Force full enrollment observation]
    Epoch -->|yes| D[Update/compare boot_id]
    D --> E{boot_id changed?}
    E -->|yes| F[Force full system_states row\nchange_reason startup\nrestart_type system_reboot]
    E -->|no| G{change_reason heartbeat-eligible?}
    G -->|heartbeat/startup/state_delta| H[Load previous system state]
    G -->|config_change/cf_deployment/other| I
    H --> J{states equivalent?}
    J -->|yes| K[Insert agent_heartbeats row only]
    J -->|no| L[Classify real transition\ninsert system_states]
```

### Equivalence check

The server compares fields that represent meaningful system identity/config:

- hostname
- store path
- OS/kernel
- hardware identifiers
- network identifiers
- secure boot/FIPS/TPM fields
- agent version/build hash
- NixOS version

It intentionally ignores fields that naturally change every heartbeat:

- timestamp
- uptime

Once the enrollment has a current full observation, an equivalent eligible
heartbeat becomes an `agent_heartbeats` row. A reboot or pending enrollment
forces a full `system_states` row even when runtime fields match history.
Migration `0318` stamps re-enrollment with `clock_timestamp()` after the row
lock and filters prior observations from current active-managed reads. Ordinary
key rotation, metadata edits and config sync preserve that epoch. Historical
full states and heartbeat links remain stored. Migration `0320` separately
preserves inactive managed detail telemetry within the epoch without restoring
Current CVE/POA&M authority.


## Related concepts

- [Agent POST types and deployment command response](../api/agent-post-types-and-deployment-response.md)
- [Authoritative system_events timeline](../data-model/system-events-timeline.md)
- [Restart and activation classification](../concepts/restart-and-activation-classification.md)
- [Web UI history rendering rules](../ui/deployment-history-rendering-rules.md)
- [Deployment history tests, debug checklist, and known pitfalls](../operations/deployment-history-debugging-and-tests.md)
- [System Deployment Status View](../data-model/views/view-system-deployment-status.md)
