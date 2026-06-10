---
id: doc-11
title: CrystalForgelatest design source index
type: specification
created_date: '2026-06-10 03:10'
tags:
  - design-parity
  - design-source
  - crystalforgelatest
  - planning
---
# CrystalForgelatest design source index

## Purpose
This document defines how `/home/mcamp/code/crystal-forge/CrystalForgelatest` should be used during backlog planning and parity execution.

## Rule
`CrystalForgelatest` is the **authoritative design reference** for UI parity work.

It should:
- be referenced from backlog tasks and parity docs
- drive visual, interaction, and screenshot expectations
- remain the single source of truth for design artifacts

It should **not** be copied wholesale into Backlog.md documents.

## Authoritative files
Top-level references:
- `/home/mcamp/code/crystal-forge/CrystalForgelatest/app.jsx`
- `/home/mcamp/code/crystal-forge/CrystalForgelatest/styles.css`
- `/home/mcamp/code/crystal-forge/CrystalForgelatest/components/`

## Design surface inventory
Primary component/view references currently present:
- `Shell.jsx`
- `DashboardView.jsx`
- `Systems.jsx`
- `SystemDetail.jsx`
- `EnvironmentsView.jsx`
- `FlakesView.jsx`
- `BuildsView.jsx`
- `EvalsView.jsx`
- `CvesView.jsx`
- `ScanningView.jsx`
- `PoliciesView.jsx`
- `ComplianceView.jsx`
- `AdminView.jsx`
- `ProfileView.jsx`
- `BuildersView.jsx`
- `CachesView.jsx`
- `HardeningTab.jsx`
- `DeployGate.jsx`
- `EvalDrawer.jsx`
- `AddSystemModal.jsx`
- `EditSystemModal.jsx`
- `SetupCoach.jsx`
- `Icon.jsx`

## How backlog tasks should use this source
Tasks should:
- reference the exact design file(s) they implement
- state whether the work is foundation, existing-surface parity, missing-surface parity, or final audit
- define acceptance criteria in objective terms tied back to the design source
- update screenshot/assertion expectations in parity docs when needed

Tasks should not:
- duplicate large sections of JSX/CSS into task descriptions
- create competing design definitions in backlog text
- treat screenshots alone as the only parity proof

## Mapping guidance
Use this source together with:
- `doc-8` — parity matrix
- `doc-9` — baseline scorecard
- `doc-10` — parity execution plan

## Maintenance rule
If `CrystalForgelatest` changes materially:
1. update the parity matrix
2. update the scorecard if priorities shift
3. update affected tasks/milestones
4. create follow-up backlog tasks for newly introduced surfaces or interactions
