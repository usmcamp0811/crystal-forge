---
id: TASK-490.14
title: >-
  Delivery: activation modes, running versus booted generation, and target
  isolation scenario
status: Backlog
assignee: []
created_date: '2026-10-17 16:37'
labels:
  - testing
  - integration
  - luna
dependencies:
  - TASK-490.12
parent_task_id: TASK-490
priority: high
type: task
ordinal: 526000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Owning check: integration-delivery. Contract ID: delivery.activation.modes. Exclusive write path: tests/integration/delivery/scenarios/activation-modes.{nix,py,cjs}.

WHY: real deploy confirmation is excluded from the legacy web-ui manifest; deployment UI steps are mocked. Seeded deployment rows cannot prove activation.

SCOPE: deployment policies manual, auto_latest, and pinned on targetA: manual requires the Deploy action and changes /run/current-system only after it; auto_latest follows a new evaluated commit; pinned ignores a newer commit. Activation strategy BootOnly changes the boot entry but not the running generation until reboot (reboot targetA and assert). Builder B also runs a second real agent with its own key: deploying targetA leaves builderB's generation and services unchanged (use a fresh output absent from builderB's store). System detail Deploy/History views show the actual running vs booted identity. Non-goals: rollback/drift (490.15).

READY WHEN: TASK-490.12 is integrated and TASK-490.7 provides targetA and builderB-agent wiring. Source to read then: cf-server models/systems.rs (DeploymentPolicy), cf-config config/deployment.rs (DeploymentStrategy), handlers/agent/*, handlers/api/systems.rs (deploy, deployment-status, generations), cf-agent deployment module; docs/knowledge/deployment/*; design SystemDetail.jsx, DeployGate.jsx.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 manual, auto_latest, and pinned activate exactly as specified on a real target
- [ ] #2 BootOnly changes the booted generation without changing the running generation until reboot
- [ ] #3 Deploying one target leaves a second real agent unchanged
- [ ] #4 UI running/booted identities match the target's actual state
- [ ] #5 Executed in real VMs with recorded evidence
<!-- AC:END -->
