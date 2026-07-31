---
id: TASK-343.2
title: 'Flakes: add/edit modal + sync/credential flow parity'
status: Backlog
assignee: []
created_date: '2026-06-10 13:33'
labels:
  - design-parity
  - flakes
  - web-ui
  - child
milestone: 'm-19: Design Parity Existing Surfaces'
dependencies: []
references:
  - TASK-343
  - /home/mcamp/code/crystal-forge/CrystalForgelatest/components/FlakesView.jsx
  - design/doc-14 - Parity-execution-playbook-agent-proof.md
documentation:
  - design/doc-8 - CrystalForgelatest-UI-Parity-Matrix-TASK-328.md
  - design/doc-14 - Parity-execution-playbook-agent-proof.md
modified_files:
  - packages/web-ui/src/views/flakes.rs
  - packages/web-ui/src/components/flake/mod.rs
  - checks/web-ui/tests/integration-test.js
priority: high
ordinal: 1742
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Child of Flakes umbrella TASK-343. Follow guide doc-14 standard procedure.

## Problem
Flakes add/edit modals and sync/credential flows must match design and call real APIs (existing steps `13e/13f/13g` cover credential paths).

## Goal
Bring Flakes add/edit modals and sync/credential interactions to parity, backed by real API actions.

## Exact scope
1. Add flake modal: create flow + credential fields call real API; validation/errors per design.
2. Edit flake modal: prefilled + save via real API; SSH/credential persistence.
3. Sync action + history-rewrite recovery match design behavior.

## Non-goals
- List/timeline layout (sibling task TASK-343.1).
- Legacy view removal (TASK-297.1 / TASK-341).

## Files
- packages/web-ui/src/views/flakes.rs
- packages/web-ui/src/components/flake/**
- checks/web-ui/tests/integration-test.js

## Verification
- nix develop -c cargo fmt -- --check
- nix develop -c cargo check --manifest-path packages/web-ui/Cargo.toml --target wasm32-unknown-unknown
- nix build .#checks.x86_64-linux.web-ui
- Reuse steps `13e-flakes-add-modal-credentials`, `13f-flakes-edit-modal-credentials`, `13g-flakes-edit-modal-ssh-save-persist`, `13h-flakes-force-push-rewrite-recovery`.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Add flake modal creates via real API with credential fields, validation, and errors
- [ ] #2 Edit flake modal prefills and saves via real API including SSH/credential persistence
- [ ] #3 Sync and history-rewrite recovery flows match the design behavior
- [ ] #4 web-ui credential/sync steps pass and assert real interactions
<!-- AC:END -->
