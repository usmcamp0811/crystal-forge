---
id: TASK-490.14
title: >-
  Delivery: activation modes, running versus booted generation, and target
  isolation scenario
status: Backlog
assignee: []
created_date: '2026-10-17 16:37'
updated_date: '2026-10-18 02:37'
labels:
  - testing
  - integration
  - luna
dependencies:
  - TASK-490.12
modified_files:
  - checks/integration-delivery/scenarios/fragments/task-490.14-activation.py
  - checks/integration-delivery/scenarios/fragments/task-490.14-activation.cjs
parent_task_id: TASK-490
priority: high
type: task
ordinal: 526000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Owning check: integration-delivery. Contract ID: delivery.activation.modes. This packet owns disjoint case fragments under checks/integration-delivery/scenarios/fragments/task-490.14-activation.py and task-490.14-activation.cjs. The coordinator owns the scenario `.nix`, Python composition, browserSpecs mapping, and explicit registration; this packet does not automatically receive a NixOS test derivation.

Prove manual, auto_latest, and pinned behavior on real targets. BootOnly has a separate required sequence: record the pre-action next-boot selection, `/run/current-system`, and `/run/booted-system`; after selecting BootOnly and before reboot, assert the next-boot selection changed while running and booted identities each remain equal to their own recorded pre-action values. Observe boot selection from the configured bootloader selection/entry, not `/run/booted-system`. After reboot, assert the target boots and runs the selected deployment. Do not assume running and booted identities were equal before the action. Verify a deployment to one target leaves a real second agent/target unchanged, with target-specific output absence proved independently.

Needs TASK-490.7 targetA and second-agent/builderB roles plus integrated build/publication code prerequisites. All target generations, rows and outputs must be produced inside the owning Delivery scenario; no producer VM state is reusable. Read the deployment API/agent and approved owner authority; as-built docs are evidence only. No production changes, helper, scenario composition, registration, CI, or docs edits. The coordinator records exact run evidence.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The coordinator composes activation fragments in the registered Delivery lifecycle scenario; no packet-specific VM derivation is added
- [ ] #2 manual, auto_latest, and pinned activate according to accepted requirements on real targets
- [ ] #3 Before BootOnly action, record boot selection, /run/current-system and /run/booted-system independently; before reboot selection changes but each running/booted identity equals its own pre-action value
- [ ] #4 Observe next-boot selection separately from /run/booted-system; after reboot the target boots and runs the selected deployment
- [ ] #5 Deploying targetA leaves the second real agent/target unchanged
- [ ] #6 The coordinator records tested revision, exact VM command, result and evidence
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-10-17 organization correction replaces prior layout/commands. Coordinator owns explicit registration/shared lib/CI/docs. Foundation commit unresolved; two existing Rust package tests block foundation; no VM execution. Backlog, not ready. Actual dependency commit(s) and verified commands required before To Do. No Luna delegation or executed slice is claimed.
<!-- SECTION:NOTES:END -->
