---
id: TASK-525
title: Implement Deploy tab design from dev commit bd8d068d
status: To Do
assignee: []
created_date: '2026-11-15T16:10:00Z'
labels:
  - web-ui
  - deployments
  - design-parity
dependencies:
  - TASK-525.1
  - TASK-449
  - TASK-415
references:
  - commit:bd8d068dc287d222baeea9c7336363a03da55371
documentation:
  - docs/design/CrystalForge/components/FlakePosition.jsx
  - docs/design/CrystalForge/components/DeployTargetPicker.jsx
priority: high
type: feature
ordinal: 572000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Umbrella for implementing the System Detail Deploy tab design from dev commit `bd8d068d` ("update deploy tab design") end to end, including the backend data that the design needs.

The design changes are in `docs/design/CrystalForge/components/FlakePosition.jsx` (new), `DeployTargetPicker.jsx` (new), `DeployGate.jsx` (compact gate), `SystemDetail.jsx` (Deploy tab composition, copying stage in the activity feed), `MgmtParts.jsx` (External/Reference overview card), `app.jsx`, and `styles.css` (`fp-*`, `tp-*`, `dg-compact`, `sd-appr-*`).

## Goal

The Deploy tab and the External/Reference overview match `bd8d068d`, and every element in them shows real server data. The UI does not show fabricated stages, approvals, attestations, peers, or build states.

## Work split

- TASK-525.1: the flake-position lane, the vertical deploy-target picker, the compact gate in the plan panel, deploy progress shown in the lane, the External/Reference overview card, and the per-system flake-position read API that includes peer hosts.
- TASK-449: the real `copying` deployment stage (agent, server, and API). The lane stage row and the activity feed show "Copying from cache" only from that work.
- TASK-415: the deployment approval requests and the signed running-state attestations. The approval row in the plan panel, the approval tags on picker rows and lane nodes, and the attestation chip with "Decide" in the flake-position card come from that work.

## Non-goals

- Deploy-gate evaluation API (TASK-353.2). The compact gate keeps the current locally derived rules until TASK-353.2 lands.
- Changes to deployment authorization, policy enforcement, or rollback eligibility.

## Completion rule

This task is Done only when TASK-525.1, TASK-449, and TASK-415 are Done and the design elements listed above render from real data.

## Risk

High overall, because it combines UI, server, and agent work. The risk is distributed across the child and dependency tasks.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 TASK-525.1 is Done
- [ ] #2 TASK-449 is Done and the lane stage row and activity feed show Copying from cache only from server-reported progress
- [ ] #3 TASK-415 is Done and the plan-panel approval row, picker and lane approval tags, and flake-position attestation chip render from real approval and attestation records
<!-- AC:END -->
