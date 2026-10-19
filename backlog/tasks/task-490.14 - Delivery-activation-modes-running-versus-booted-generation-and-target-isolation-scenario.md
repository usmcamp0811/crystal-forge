---
id: TASK-490.14
title: >-
  Delivery: activation modes, running versus booted generation, and target
  isolation scenario
status: Backlog
assignee: []
created_date: '2026-10-17 16:37'
updated_date: '2026-10-19 02:13'
labels:
  - testing
  - integration
  - luna
dependencies:
  - TASK-490.12
modified_files:
  - checks/integration-delivery/scenarios/fragments/activation.py
  - checks/integration-delivery/scenarios/fragments/activation.cjs
parent_task_id: TASK-490
priority: high
type: task
ordinal: 526000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Owning check: integration-delivery. Contract ID: delivery.activation.modes. Goal: prove real activation policies, BootOnly semantics and target isolation; mocked UI or seeded deployment rows are insufficient. Risk: high (boot and identity assertions). Non-goals: rollback/drift (.15), product changes, shared helper/composition/registration/CI/docs edits.

Writable paths (only):
- checks/integration-delivery/scenarios/fragments/activation.py
- checks/integration-delivery/scenarios/fragments/activation.cjs

Composition target: checks/integration-delivery/scenarios/delivery-lifecycle.nix. Coordinator owns Nix/Python composition, browserSpecs mapping and explicit owner registration/aggregation. No packet-specific VM derivation. Earlier real builds/publications/deployments in this same fresh Delivery VM/database MAY be consumed through receipts. Another derivation's live targets, rows, outputs or receipts MUST NOT be consumed.

manual changes /run/current-system only after Deploy; auto_latest follows a newly evaluated commit; pinned ignores newer commits. BootOnly sequence: before action record next-boot selection, /run/current-system and /run/booted-system independently. Before reboot selection changes, while running and booted identities each equal their own pre-action value. Observe next-boot selection from the configured bootloader selection/entry, separately from /run/booted-system; do not assume running and booted were initially equal. After reboot the target boots and runs the selected deployment. System detail Deploy/History must display actual running versus booted identity.

A second real agent on builderB uses its own key. Deploying targetA leaves that second target's generation/services unchanged; prove the target-specific output independently absent in builderB's store before the isolation operation. Expected identities come from authored facts/target observations and same-run receipts, not CF response data under test.

Dependencies/readiness: TASK-490.12 publication and its .10/.7 build/role prerequisites must be integrated at actual tested revisions; .7 supplies bootable targetA and builderB/second-agent wiring. Foundation bba718befbc183863781656df2df272e5f735d5a on origin/dev 5ae7feab passed with .25/.26; it does not prove activation or future roles. This packet remains unexecuted and awaits selection, these concrete code interfaces and accepted policy/boot UI authority.

Read-only source/authority: cf-server models/systems.rs DeploymentPolicy, cf-config config/deployment.rs DeploymentStrategy, handlers/agent/*, api/systems.rs deploy/status/generations, cf-agent deployment; docs/knowledge/deployment/*; approved SystemDetail.jsx/DeployGate.jsx. As-built D1 is evidence, not acceptance authority; uncertain cases stay blocked pending owner decision.

Focused verification after composition: `nix build .#checks.x86_64-linux.integration-delivery.scenarios.delivery-lifecycle -L`. Coordinator records exact tested revision, command, result and evidence. Retain correct failures and linked product defects; escalate helper/authority gaps without weakening assertions.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 manual, auto_latest and pinned follow accepted requirements on real targetA, including manual Deploy and pinned ignoring newer evaluated commits
- [ ] #2 BootOnly records next-boot selection, /run/current-system and /run/booted-system independently before action; before reboot selection changes while each running/booted identity equals its own recorded pre-action value; after reboot the target boots and runs the selected deployment; next-boot selection is observed separately through configured bootloader selection/entry, never inferred from /run/booted-system or assumed pre-action identity equality
- [ ] #3 Deploying targetA leaves the second real agent on builderB and its generation/services unchanged, with target-specific output absence independently proved
- [ ] #4 System detail Deploy/History running/booted identities match independent observations of the real target
- [ ] #5 Coordinator composes only the named activation fragments into delivery-lifecycle after real same-scenario build/publication setup via receipts; no packet-specific derivation or cross-derivation live-state reuse occurs
- [ ] #6 Coordinator executes the focused delivery-lifecycle gate and records exact tested revision, command, result and evidence
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Current readiness: foundation bba718be passed; .12 publication and .7/.10 role/build interfaces plus accepted activation authority remain packet prerequisites. No activation VM result is claimed.

### Historical
2026-10-17 review recorded unresolved foundation/package prerequisites and no VM execution at that time. Former BootOnly wording conflated boot selection with booted identity; the current AC #2 records each identity independently. The former per-packet activation derivation is no longer the composition model.
<!-- SECTION:NOTES:END -->
