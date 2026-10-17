---
id: TASK-490.15
title: 'Delivery: rollback, drift, and current-version restart recovery scenario'
status: Backlog
assignee: []
created_date: '2026-10-17 16:37'
labels:
  - testing
  - integration
  - luna
dependencies:
  - TASK-490.14
parent_task_id: TASK-490
priority: medium
type: task
ordinal: 527000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Owning check: integration-delivery. Contract ID: delivery.activation.recovery. Exclusive write path: tests/integration/delivery/scenarios/activation-recovery.{nix,py,cjs}.

WHY: rollback preselection and drift are proven only by mocked steps (12o, 12i-12k); restart recovery is proven only by Rust state-machine tests.

SCOPE: rollback from the System detail view restores the previous generation on targetA (actual /run/current-system and boot entry); an out-of-band local change on targetA is reported as drift with the actual running identity; restart of the server during an evaluation/build and of the agent during a deployment recovers to a consistent state at the current version (unit_killed, then wait_for the specified state). Non-goals: version-transition recovery (Upgrade, 490.23).

READY WHEN: TASK-490.14 is integrated. Source to read then: cf-server handlers/api/systems.rs (rollback, rollback-generation, sync, history, agent-events), reset_non_terminal_derivations in bin/server.rs startup, cf-agent system_state.rs; docs/knowledge/deployment/*, docs/knowledge/ui/deployment-history-rendering-rules.md; design SystemDetail.jsx.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 UI rollback restores the previous generation on the real target
- [ ] #2 Out-of-band changes are reported as drift with the actual identity
- [ ] #3 Server and agent restarts during work recover to the specified state
- [ ] #4 Executed in real VMs with recorded evidence
<!-- AC:END -->
