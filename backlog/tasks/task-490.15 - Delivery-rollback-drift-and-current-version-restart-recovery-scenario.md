---
id: TASK-490.15
title: 'Delivery: rollback, drift, and current-version restart recovery scenario'
status: To Do
assignee: []
created_date: '2026-10-17 16:37'
updated_date: '2026-10-20 03:13'
labels:
  - testing
  - integration
  - luna
dependencies:
  - TASK-490.14
modified_files:
  - checks/integration-delivery/scenarios/fragments/task-490.15-recovery.py
  - checks/integration-delivery/scenarios/fragments/task-490.15-recovery.cjs
parent_task_id: TASK-490
priority: medium
type: task
ordinal: 527000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Owning check: integration-delivery. Contract ID: delivery.activation.recovery. This packet owns only recovery Python/browser fragments under `checks/integration-delivery/scenarios/fragments/task-490.15-*`. Coordinator owns `delivery-lifecycle.nix`, composition and registration; no packet-specific derivation.

Compose after actual deployment generations and pending work created by TASK-490.14 cases in the same fresh Delivery VM/target. Prove rollback to prior target generation, independent drift detection, and bounded recovery after server/builder/agent restart during work. Reuse same-run target/job receipts; do not depend on TASK-490.14's separate run state. Version transition remains TASK-490.23 under the Upgrade owner.

Requires TASK-490.7/.14 role and code prerequisites, accepted authority, and passing foundation/.25/.26. Writable paths are named fragments only. Production, shared helper, scenario composition/registration, CI and docs are read-only. Coordinator records one Delivery VM result/evidence.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 UI rollback restores the previous generation on the real target
- [ ] #2 Out-of-band changes are reported as drift with actual running identity
- [ ] #3 Server/builder/agent restarts during work recover to specified state
- [ ] #4 The coordinator composes this fragment after .14 in delivery-lifecycle; no packet derivation is added
- [ ] #5 The coordinator records actual Delivery VM revision, command, outcome and evidence
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-10-17 organization correction: native owner-local layout and explicit coordinator registration. Foundation dependency commit unresolved; foundation blocked by two existing Rust package tests; no VM execution. Backlog, not ready. Actual dependency commit(s) and verified commands required before To Do. No Luna delegation or executed slice is claimed.

Scenario isolation correction: TASK-490.14's generation/output/VM cannot be reused. Set up and perform the prior real deployment(s) in this scenario's own fresh targetA VM before exercising rollback and drift; reuse only immutable fixture definitions or role/helper code. Current foundation VM execution fails on TASK-490.25/.26, so remain Backlog.

Owner explicitly selected retained-generation rollback after two actual CF-managed manual deployments in the same Delivery lifecycle. No drift/restart-recovery completion is implied. Use semantic recovery.py/.cjs filenames, not historical task-numbered paths. Coordinator owns shared fixture/composition.
<!-- SECTION:NOTES:END -->
