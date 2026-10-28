---
type: UI Design
title: "Web UI history rendering rules"
description: "Defines History lifecycle presentation, distinct deployment-attempt counts, event identity, and bounded legacy compatibility."
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
| `cf_deployment` | `DeployStarted` / `Deploy` / `DeployFailed` depending on outcome | visible |
| `local_rebuild` | `LocalRebuildMatched` or `LocalRebuildUntracked` | visible |
| `restart` | `Restart` | visible, clusterable |
| `agent_restart` | `AgentRestart` | visible, not clustered |
| `state_change` | `StateChange` | hidden from Deployment History |

New event-backed rows also carry `event_type`, generation/store-path deltas,
actor/source, deployment identifiers, timestamps, `correlation_id`, and metadata.
New fields are optional/defaulted on the wire so older clients and older history
rows remain compatible.

## Deployment observations and counts

TASK-500 repairs presentation against `SystemDetail.jsx` at `df11c800`, with
the authoritative persisted lifecycle taking precedence over the mock data.
`cf_deployment_started` maps to `DeployStarted`, even if another payload field
suggests success. The card says **Deployment started**, **started · nonterminal**,
and **activation not yet confirmed**. These describe that observation, not the
latest state of the deployment. A later terminal observation remains a separate
card. Started has no activation generation, generation transition, or rollback
action. It does not decrement the legacy generation fallback and does not produce
reconstructed completion log lines.

`cf_deployment_succeeded` maps to `Deploy`; `cf_deployment_failed` maps to
`DeployFailed`. Explicit event types take precedence over kind/outcome and text
heuristics. For older payloads, `cf_deployment` with outcome `started` also maps
to `DeployStarted`. The legacy text fallback recognizes outcome `started` or
reason `cf_deployment_started` before failure and rebuild heuristics.

The **deploys** metric counts CF deployment attempts and local rebuilds in the
loaded history window. It is not a count of successful activations or timeline
cards. This preserves the approved reference's `type: "deploy"` category, which
includes failed CF attempts and both matched and untracked local rebuilds.

- CF observations with the same `deployment_id` count once, including a
  Started-only attempt, Started plus success, or Started plus failure.
- Repeated persisted event `id` values produce one card and count once.
- Each local rebuild counts once after event-ID deduplication.
- Without `deployment_id`, each distinct observation counts once. The UI does
  not join rows by commit, store path, generation, or timestamp. Those values
  cannot distinguish a retry from another lifecycle observation.
- Without event IDs, legacy rows remain separate. History and reconstructed Logs
  retain positional `ev{index}` anchors for these rows only.
- Persisted event IDs anchor History cards and their Logs jump. Cards expose
  `data-event-id` and, when present, `data-deployment-id` for exact identification.
- Counts derive from the response on each render. Reload does not depend on a
  client-side accumulator. The metric describes the returned window, not an
  inferred all-time total.

The rollback plan displays Strategy **switch_to_generation**, as specified by
the approved `DeployTab` previous-generation state. This presentation label does
not change the rollback API operation.

Backwards compatibility rule:

> If a system has no `system_events` rows yet, the history API may fall back to
> legacy `system_states` reconstruction. Once event rows exist for a system, the
> Deployment History timeline uses `system_events` as the authoritative source.

Important UI rule:

> `state_change` must not fall through to legacy classification. Otherwise raw
> `state_delta` text can be misread as a local rebuild and flood Deployment
> History with fake rebuild rows.

## Current limitation: failed deployment history

Failure presentation depends on available failure evidence. Missing terminal
events cannot identify the final result of an attempt.

The server event model and query layer support persisted
`cf_deployment_failed`; the History API maps that type to outcome `failed`.
The UI renders that explicit type as failure even if its outcome text differs.
Legacy failure-like outcomes containing `fail` or `error` remain supported.
An absent terminal event does not prove success or failure. The UI must not
invent completion evidence for a Started observation.


## Related concepts

- [Authoritative system_events timeline](../data-model/system-events-timeline.md)
- [Restart and activation classification](../concepts/restart-and-activation-classification.md)
- [Agent heartbeat, state, deployment, and history logic](../deployment/agent-heartbeat-vs-state-persistence.md)
