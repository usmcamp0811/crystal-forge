---
id: TASK-336.1
title: 'Admin: view parity + real administrative data flows'
status: Backlog
assignee: []
created_date: '2026-06-10 13:35'
labels:
  - design-parity
  - admin
  - web-ui
  - child
milestone: 'm-20: Design Parity Missing Surfaces'
dependencies: []
references:
  - TASK-336
  - /home/mcamp/code/crystal-forge/CrystalForgelatest/components/AdminView.jsx
  - design/doc-14 - Parity-execution-playbook-agent-proof.md
documentation:
  - design/doc-8 - CrystalForgelatest-UI-Parity-Matrix-TASK-328.md
  - design/doc-14 - Parity-execution-playbook-agent-proof.md
modified_files:
  - packages/web-ui/src/views/admin.rs
  - packages/default/src
  - checks/web-ui/tests/integration-test.js
priority: high
ordinal: 1671
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Child of Admin umbrella TASK-336. Follow guide doc-14 standard procedure.

## Problem
The Admin view (`views/admin.rs`, route `/admin`) must match `CrystalForgelatest/components/AdminView.jsx` with real administrative data and actions.

## Goal
Bring the Admin view to parity: structure, tables/controls/dialogs, and backend-driven data/outcomes.

## Exact scope
1. Admin page structure + sections match design.
2. Admin tables/controls/dialogs match design interaction behavior.
3. Displayed values and action results are backend-driven (no placeholders in production).
4. Loading/empty/error/success states match design.

## Non-goals
- IAM redesign beyond what the Admin surface needs.

## Files
- packages/web-ui/src/views/admin.rs
- packages/default/src (only if a required admin endpoint/field is missing)
- checks/web-ui/tests/integration-test.js

## Verification
- nix develop -c cargo fmt -- --check
- nix develop -c cargo check --manifest-path packages/web-ui/Cargo.toml --target wasm32-unknown-unknown
- nix build .#checks.x86_64-linux.web-ui
- Add an admin screenshot/assertion step (no dedicated admin step exists yet).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Admin page structure and sections match the design
- [ ] #2 Admin tables/controls/dialogs match design interaction behavior
- [ ] #3 Displayed values and action results are backend-driven with no production placeholders
- [ ] #4 Loading/empty/error/success states match the design
- [ ] #5 A web-ui admin step screenshots the view and asserts a real interaction
<!-- AC:END -->
