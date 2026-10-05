---
type: Data Model
title: "Authoritative system_events timeline"
description: "Defines the system_events event types, idempotent dedupe keys, deterministic ordering and correlation, and the pending_system_deployments context that attributes Crystal Forge deployments; open it when reading or changing Deployment History data."
tags:
  - crystal-forge
  - system-events
  - deployment-history
  - pending-deployment
  - out-of-band-deployment
implementation_status: implemented
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-03T22:55:03-05:00
sources:
  - id: origin
    resource: "Crystal Forge repository file docs/agent-heartbeat-state-history.md at commit 3b23d36f"
    title: "Agent heartbeat, state, deployment, and history logic"
---
# Authoritative `system_events` timeline

The server appends user-facing timeline events to `system_events` only when an
incoming report proves a real transition. Raw state reports and heartbeat-equivalent
metadata updates must not become Deployment History entries.

Supported event types:

| `event_type` | Meaning | UI classification |
| --- | --- | --- |
| `cf_deployment_succeeded` | Reported store path matched a pending Crystal Forge desired target | Crystal Forge deployment |
| `cf_deployment_failed` | Reserved for reliable server-side failure attribution | Failed deployment |
| `local_rebuild_detected` | Generation/store path changed without matching pending CF context | Local rebuild |
| `system_reboot` | `boot_id` changed | System restart |
| `agent_restart` | startup report on same boot without generation/store-path change | Agent restart |

Events are idempotent through a durable unique key:

```sql
UNIQUE (system_id, event_type, dedupe_key)
```

Example dedupe keys:

- `system_reboot:<new_boot_id>`
- `agent_restart:<boot_id>:<store_path>`
- `local_rebuild:<old_generation>:<old_store_path>-><new_generation>:<new_store_path>`
- `cf_deployment_succeeded:<pending_deployment_id>`

Ordering is deterministic:

```sql
ORDER BY occurred_at DESC, observed_at DESC, correlation_id DESC, event_rank ASC, id DESC
```

Events emitted from the same report share a `correlation_id`, so a report that
both changes generation and changes `boot_id` can be grouped later without relying
only on timestamps. `event_rank` provides a stable causal order inside one report:
configuration/deployment transitions (rank 10) render before `system_reboot`
(rank 20), which renders before `agent_restart` (rank 30).

## Pending deployment context

Detached activation via `systemd-run --no-block` can restart the agent before the
agent posts a `cf_deployment` state row. To preserve attribution, the server stores
pending Crystal Forge deployment context when a desired store-path target is set.

`pending_system_deployments` records:

- `system_id`
- `target_store_path`
- `status` (`pending`, `succeeded`, `failed`, `superseded`, `expired`)
- `issued_at` / `expires_at` / `completed_at`
- `source`
- metadata

When a later heartbeat/state report observes `store_path == target_store_path`, the
server emits `cf_deployment_succeeded` and marks the pending context `succeeded`.
When a newer desired target is set, older pending contexts for the same system are
marked `superseded`. Pending contexts expire after a bounded window so stale targets
cannot claim unrelated future host changes.

Only live `pending` contexts may attribute future reports. Once a context is marked
`succeeded`, it is no longer matchable; a later manual switch back to the same store
path is therefore classified as `local_rebuild_detected`.

Commit-based deploy requests are resolved to the matching NixOS derivation store
path before `systems.desired_target` is set. If no store path or expected store path
is available for that commit/configuration, the deploy request is rejected instead
of sending an agent a commit SHA it cannot activate.

`cf_deployment_failed` is part of the event contract, but the current server path
does not always receive reliable post-detached failure data. Until that data is
persisted reliably, failed detached activations may remain absent from
`system_events` instead of being guessed from raw reports.

> **Status:** Current limitation documented in the source as of commit 3b23d36f: `cf_deployment_failed` is reserved and failed detached activations may be absent from `system_events`. This migration did not verify it against the code. `cf_deployment_failed` appears in `packages/default/crates/cf-server/src/models/system_events.rs` and `packages/default/crates/cf-server/src/handlers/api/systems.rs`, so a later verification pass must check whether failure events are now recorded. The tables `system_events` and `pending_system_deployments` are created in `packages/default/crates/cf-server/migrations/0155_system_events_timeline.sql`.


## Related concepts

- [Agent heartbeat, state, deployment, and history logic](../deployment/agent-heartbeat-vs-state-persistence.md)
- [Agent POST types and deployment command response](../api/agent-post-types-and-deployment-response.md)
- [Restart and activation classification](../concepts/restart-and-activation-classification.md)
- [Web UI history rendering rules](../ui/deployment-history-rendering-rules.md)
- [Commit Deployment Timeline View](views/view-commit-deployment-timeline.md)
