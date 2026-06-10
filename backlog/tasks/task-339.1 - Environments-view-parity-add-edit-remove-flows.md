---
id: TASK-339.1
title: 'Environments: view parity + add/edit/remove flows'
status: Backlog
assignee: []
created_date: '2026-06-10 13:35'
labels:
  - design-parity
  - environments
  - web-ui
  - child
milestone: 'm-20: Design Parity Missing Surfaces'
dependencies: []
references:
  - TASK-339
  - >-
    /home/mcamp/code/crystal-forge/CrystalForgelatest/components/EnvironmentsView.jsx
  - design/doc-14 - Parity-execution-playbook-agent-proof.md
documentation:
  - design/doc-8 - CrystalForgelatest-UI-Parity-Matrix-TASK-328.md
  - design/doc-14 - Parity-execution-playbook-agent-proof.md
modified_files:
  - packages/web-ui/src/views/environments.rs
  - packages/web-ui/src/components/environments/mod.rs
  - checks/web-ui/tests/integration-test.js
parent_task_id: TASK-339
priority: high
ordinal: 1711
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Child of Environments umbrella TASK-339. Follow guide doc-14 standard procedure.

## Problem
The Environments view (`views/environments.rs`) must match `CrystalForgelatest/components/EnvironmentsView.jsx`, including environment cards, color theming, and add/edit/remove flows.

## Goal
Bring the Environments view + its modals to parity, backed by real API CRUD.

## Exact scope
1. Environments list/cards + color theming match design.
2. Add/edit environment modals + remove dialog match design and use real API.
3. Loading/empty/error states match design.

## Non-goals
- Cache destination assignment work (tracked separately: TASK-175/178/179/180).

## Files
- packages/web-ui/src/views/environments.rs
- packages/web-ui/src/components/environments/**
- checks/web-ui/tests/integration-test.js

## Verification
- nix develop -c cargo fmt -- --check
- nix develop -c cargo check --manifest-path packages/web-ui/Cargo.toml --target wasm32-unknown-unknown
- nix build .#checks.x86_64-linux.web-ui
- Reuse steps `14-environments`, `14b-environments-config-warning`.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Environments list/cards + color theming match the design
- [ ] #2 Add/edit environment modals + remove dialog match design and use the real API
- [ ] #3 Loading/empty/error states match the design
- [ ] #4 Steps 14-environments and 14b-environments-config-warning pass with parity assertions
<!-- AC:END -->
