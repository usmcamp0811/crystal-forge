---
id: TASK-345.1
title: 'Evaluations: queue/table + detail drawer parity'
status: Backlog
assignee: []
created_date: '2026-06-10 13:33'
labels:
  - design-parity
  - evaluations
  - web-ui
  - child
milestone: 'm-19: Design Parity Existing Surfaces'
dependencies: []
references:
  - TASK-345
  - /home/mcamp/code/crystal-forge/CrystalForgelatest/components/EvalsView.jsx
  - /home/mcamp/code/crystal-forge/CrystalForgelatest/components/EvalDrawer.jsx
  - design/doc-14 - Parity-execution-playbook-agent-proof.md
documentation:
  - design/doc-8 - CrystalForgelatest-UI-Parity-Matrix-TASK-328.md
  - design/doc-14 - Parity-execution-playbook-agent-proof.md
modified_files:
  - packages/web-ui/src/views/evaluations.rs
  - packages/web-ui/src/components/eval_log_modal.rs
  - packages/web-ui/assets/app.css
  - checks/web-ui/tests/integration-test.js
parent_task_id: TASK-345
priority: high
ordinal: 1761
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Child of Evaluations umbrella TASK-345. Follow guide doc-14 standard procedure.

## Problem
The Evaluations view (`views/evaluations.rs`) must match `CrystalForgelatest/components/EvalsView.jsx` and `EvalDrawer.jsx` for queue/table layout and the detail drawer.

## Goal
Pixel-align the Evaluations queue/table, selection behavior, and detail drawer (policy matrix, dependency graph, live logs) to the design, backed by real API/websocket data.

## Exact scope
1. Queue/table density, columns, selection highlight, first-row auto-select match design.
2. Detail drawer matches EvalDrawer.jsx (policy matrix, dependency graph, real log streaming).
3. Ordering/cancel controls work against the real API.

## Non-goals
- Builds view (sibling surface TASK-347).
- Shared coherence-only changes covered by TASK-275 (coordinate, don't duplicate).

## Files
- packages/web-ui/src/views/evaluations.rs
- packages/web-ui/src/components/eval_log_modal.rs
- packages/web-ui/assets/app.css
- checks/web-ui/tests/integration-test.js

## Verification
- nix develop -c cargo fmt -- --check
- nix develop -c cargo check --manifest-path packages/web-ui/Cargo.toml --target wasm32-unknown-unknown
- nix build .#checks.x86_64-linux.web-ui
- Extend steps `26-evaluations` and `26b-evaluations-history`.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Evaluations queue/table density, columns, selection, and first-row auto-select match the design
- [ ] #2 Detail drawer matches EvalDrawer.jsx (policy matrix, dependency graph, live logs)
- [ ] #3 Ordering/cancel controls operate against the real API
- [ ] #4 web-ui steps screenshot queue + drawer and assert selection and a real control
<!-- AC:END -->
