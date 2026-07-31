---
id: TASK-346.1
title: 'Builders: view parity + add/edit builder modal parity'
status: Backlog
assignee: []
created_date: '2026-06-10 13:34'
labels:
  - design-parity
  - builders
  - web-ui
  - child
milestone: 'm-0: Critical Bugs & Stability'
dependencies: []
references:
  - TASK-346
  - >-
    /home/mcamp/code/crystal-forge/CrystalForgelatest/components/BuildersView.jsx
  - design/doc-14 - Parity-execution-playbook-agent-proof.md
documentation:
  - design/doc-8 - CrystalForgelatest-UI-Parity-Matrix-TASK-328.md
  - design/doc-14 - Parity-execution-playbook-agent-proof.md
modified_files:
  - packages/web-ui/src/views/builders.rs
  - packages/web-ui/src/components/builders/mod.rs
  - checks/web-ui/tests/integration-test.js
priority: high
ordinal: 1791
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Child of Builders umbrella TASK-346. Follow guide doc-14 standard procedure.

## Problem
The Builders view (`views/builders.rs`) must match `CrystalForgelatest/components/BuildersView.jsx`, including builder cards/rows, metrics, and add/edit modals.

## Goal
Bring the Builders list + add/edit modals to parity, backed by real API CRUD. (RBAC 403 bug is tracked separately by TASK-204.)

## Exact scope
1. Builders list (cards/rows) + metrics view match design.
2. Add/edit builder modal fields (name/status/max_concurrent_jobs/cpu/mem/env assignments/key rotation) match design and save via real API.
3. Loading/empty/error states match design.

## Non-goals
- 403/RBAC access bug (TASK-204).
- Builder runtime concurrency behavior (TASK-291).

## Files
- packages/web-ui/src/views/builders.rs
- packages/web-ui/src/components/builders/**
- checks/web-ui/tests/integration-test.js

## Verification
- nix develop -c cargo fmt -- --check
- nix develop -c cargo check --manifest-path packages/web-ui/Cargo.toml --target wasm32-unknown-unknown
- nix build .#checks.x86_64-linux.web-ui
- Reuse steps `11b-builders` and `11c-builders-edit-modal`.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Builders list (cards/rows) + metrics match the design
- [ ] #2 Add/edit builder modal fields match design and save via real API
- [ ] #3 Loading/empty/error states match the design
- [ ] #4 Steps 11b-builders and 11c-builders-edit-modal pass with parity assertions
<!-- AC:END -->
