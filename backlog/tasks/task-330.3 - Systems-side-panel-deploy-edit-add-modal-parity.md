---
id: TASK-330.3
title: 'Systems: side panel + deploy/edit/add modal parity'
status: Backlog
assignee: []
created_date: '2026-06-10 13:29'
labels:
  - design-parity
  - systems
  - web-ui
  - child
milestone: 'm-19: Design Parity Existing Surfaces'
dependencies: []
references:
  - TASK-330
  - /home/mcamp/code/crystal-forge/CrystalForgelatest/app.jsx
  - >-
    /home/mcamp/code/crystal-forge/CrystalForgelatest/components/EditSystemModal.jsx
  - >-
    /home/mcamp/code/crystal-forge/CrystalForgelatest/components/AddSystemModal.jsx
  - design/doc-14 - Parity-execution-playbook-agent-proof.md
documentation:
  - design/doc-8 - CrystalForgelatest-UI-Parity-Matrix-TASK-328.md
  - design/doc-14 - Parity-execution-playbook-agent-proof.md
modified_files:
  - packages/web-ui/src/views/systems_list.rs
  - packages/web-ui/src/components/system/deploy_system_modal.rs
  - packages/web-ui/src/components/system/edit_system_modal.rs
  - packages/web-ui/src/components/forms/add_system_form.rs
  - packages/web-ui/assets/app.css
  - checks/web-ui/tests/integration-test.js
parent_task_id: TASK-330
priority: high
ordinal: 1623
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Child of Systems umbrella TASK-330. Follow guide doc-14 standard procedure.

## Problem
The Systems detail side panel and the deploy/edit/add modals must match the design interactions and visuals in `CrystalForgelatest/app.jsx` (SystemPanel, DeployModal, EditSystemModal, AddSystemModal).

## Goal
Bring the Systems side panel and its three modals to visual + interaction parity, backed by real API actions.

## Exact scope
1. Side panel: open on card/row click; sections, spacing, tag clicks, and action buttons match design; close behavior matches.
2. Deploy modal: commit selection + deploy action calls the real API; loading/success/error handled.
3. Edit modal: prefilled fields; save calls real API; validation + error display.
4. Add modal: create flow calls real API; validation + error display.
5. Backdrop/z-index/animation match design.

## Non-goals
- List/stat/filter layout (sibling task).
- Mock removal (sibling task).

## Files
- packages/web-ui/src/views/systems_list.rs
- packages/web-ui/src/components/system/deploy_system_modal.rs
- packages/web-ui/src/components/system/edit_system_modal.rs
- packages/web-ui/src/components/forms/add_system_form.rs
- packages/web-ui/assets/app.css
- checks/web-ui/tests/integration-test.js

## Verification
- nix develop -c cargo fmt -- --check
- nix develop -c cargo check --manifest-path packages/web-ui/Cargo.toml --target wasm32-unknown-unknown
- nix build .#checks.x86_64-linux.web-ui
- Extend steps `12e-systems-edit-modal` and `12f-systems-deploy-modal`; add a side-panel-open screenshot/assertion step.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Side panel opens from card/row click and matches design sections/spacing/actions
- [ ] #2 Deploy modal commit selection and deploy use real API with loading/success/error states
- [ ] #3 Edit modal prefills, saves via real API, and shows validation/errors
- [ ] #4 Add modal creates via real API with validation/errors
- [ ] #5 web-ui steps capture side panel + each modal and assert a real open/submit interaction
<!-- AC:END -->
