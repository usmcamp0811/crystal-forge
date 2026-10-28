---
type: Runbook
title: "Deployment history tests, debug checklist, and known pitfalls"
description: "Lists the tests that protect deployment history classification, a debug checklist for unexpected history rows, and known pitfalls such as treating every state_delta as a rebuild."
tags:
  - crystal-forge
  - deployment-history
  - debugging
  - tests
  - pitfalls
implementation_status: implemented
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-03T22:55:03-05:00
sources:
  - id: origin
    resource: "Crystal Forge repository file docs/agent-heartbeat-state-history.md at commit 3b23d36f"
    title: "Agent heartbeat, state, deployment, and history logic"
---
# Deployment history tests, debug checklist, and known pitfalls

This runbook belongs to the agent heartbeat and deployment history logic described in [Agent heartbeat, state, deployment, and history logic](../deployment/agent-heartbeat-vs-state-persistence.md).

## Correct tests to keep

Tests should assert the behavior we actually want:

- `state_delta` with unchanged state is heartbeat-eligible server-side
- `state_delta` with changed generation/store path becomes `local_rebuild`
- `startup` with same boot and changed generation/store path becomes `local_rebuild`
- `startup` with `restart_type = system_reboot` stays `restart`
- authoritative UI `event_kind = state_change` becomes `StateChange`
- UI `StateChange` entries are excluded from Deployment History
- authoritative UI `event_kind = local_rebuild` still renders as Local rebuild

Tests should **not** assert that generic legacy `state_delta` or `state_change`
always means Local rebuild. That assertion is too broad and causes the false
positive history spam seen on `mattis` and `reckless`.

## Debug checklist

When a system shows unexpected history rows:

1. Check the agent journal for deploy activity.
   - `No desired target in heartbeat response` + `No deployment needed` means
     no CF deployment happened.
2. Compare repeated history rows.
   - Same generation and same store path every 10 minutes means heartbeat rows
     are being misclassified or incorrectly inserted as `system_states`.
3. Check backend `event_kind` returned by `/api/v1/systems/{system_id}/history`.
   - `state_change` should not render as Local rebuild.
   - `local_rebuild` should only appear for real generation/store-path changes.
4. Check agent version behavior.
   - Older agents may send `state_delta` every heartbeat.
   - The server must handle that compatibly by equivalence-checking it.
5. For `nixos-rebuild switch`, expect an agent restart.
   - If the generation/store path changed, classify the row as Local rebuild.
   - If generation/store path did not change, classify as Agent restarted or
     State change, not Local rebuild.

## Known pitfalls

- Do not infer Crystal Forge deployment from UI text alone. Confirm the agent
  received a `desired_target` and executed deployment logic.
- Do not treat all `state_delta` rows as rebuilds. Only generation/store-path
  changes are rebuilds.
- Do not let authoritative `state_change` fall through to legacy heuristics.
- Do not hide a real `nixos-rebuild switch` behind `Agent restarted` just
  because systemd restarted the agent during activation.
- Do not change already-applied migrations to repair history behavior; fix
  forward with code or a new migration as appropriate.

## Related concepts

- [Restart and activation classification](../concepts/restart-and-activation-classification.md)
- [Authoritative system_events timeline](../data-model/system-events-timeline.md)
- [Web UI history rendering rules](../ui/deployment-history-rendering-rules.md)
