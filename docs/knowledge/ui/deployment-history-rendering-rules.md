---
type: UI Design
title: "Web UI history rendering rules"
description: "Defines how the Web UI maps backend event_kind and event_type values to Deployment History entries, the backwards-compatibility fallback to system_states, and the documented failed-deployment limitation."
tags:
  - crystal-forge
  - web-ui
  - deployment-history
  - system-detail
  - event-kind
implementation_status: implemented
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-03T22:55:03-05:00
sources:
  - id: origin
    resource: "Crystal Forge repository file docs/agent-heartbeat-state-history.md at commit 3b23d36f"
    title: "Agent heartbeat, state, deployment, and history logic"
---
# Web UI history rendering rules

The UI should prefer authoritative backend `event_type`/`event_kind` over legacy
text heuristics.

Expected mappings:

| backend `event_kind` | UI event kind | Timeline visibility |
| --- | --- | --- |
| `cf_deployment` | `Deploy` / `DeployFailed` depending on outcome | visible |
| `local_rebuild` | `LocalRebuildMatched` or `LocalRebuildUntracked` | visible |
| `restart` | `Restart` | visible, clusterable |
| `agent_restart` | `AgentRestart` | visible, not clustered |
| `state_change` | `StateChange` | hidden from Deployment History |

New event-backed rows also carry `event_type`, generation/store-path deltas,
actor/source, deployment identifiers, timestamps, `correlation_id`, and metadata.
New fields are optional/defaulted on the wire so older clients and older history
rows remain compatible.

Backwards compatibility rule:

> If a system has no `system_events` rows yet, the history API may fall back to
> legacy `system_states` reconstruction. Once event rows exist for a system, the
> Deployment History timeline uses `system_events` as the authoritative source.

Important UI rule:

> `state_change` must not fall through to legacy classification. Otherwise raw
> `state_delta` text can be misread as a local rebuild and flood Deployment
> History with fake rebuild rows.

## Current limitation: failed deployment history

> **Status:** Current limitation documented in the source as of commit 3b23d36f; this migration did not compare it with the code. The UI model and the server event type `cf_deployment_failed` both appear in `packages/web-ui/src/views/system_detail.rs`, so a later verification pass must check whether `DeployFailed` is now fed by persisted failure events. See [Authoritative system_events timeline](../data-model/system-events-timeline.md).

`DeployFailed` only appears when the history/API payload includes a failure-like
outcome (for example a value containing `fail` or `error`). The current
heartbeat/system-state history path emits `outcome = recorded` for normal state
rows, and an agent-side `DeploymentResult::Failed` is not automatically converted
into a failed deployment history row unless that failure is also persisted and
merged into the history stream. Treat `DeployFailed` as supported by the UI model
but dependent on a failure outcome being present in the data.


## Related concepts

- [Authoritative system_events timeline](../data-model/system-events-timeline.md)
- [Restart and activation classification](../concepts/restart-and-activation-classification.md)
- [Agent heartbeat, state, deployment, and history logic](../deployment/agent-heartbeat-vs-state-persistence.md)
