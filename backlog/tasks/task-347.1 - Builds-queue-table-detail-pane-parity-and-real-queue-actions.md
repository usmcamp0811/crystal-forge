---
id: TASK-347.1
title: 'Builds: queue/table + detail pane parity and real queue actions'
status: Backlog
assignee: []
created_date: '2026-06-10 13:33'
labels:
  - design-parity
  - builds
  - web-ui
  - child
milestone: 'm-19: Design Parity Existing Surfaces'
dependencies: []
references:
  - TASK-347
  - /home/mcamp/code/crystal-forge/CrystalForgelatest/components/BuildsView.jsx
  - design/doc-14 - Parity-execution-playbook-agent-proof.md
documentation:
  - design/doc-8 - CrystalForgelatest-UI-Parity-Matrix-TASK-328.md
  - design/doc-14 - Parity-execution-playbook-agent-proof.md
modified_files:
  - packages/web-ui/src/views/builds.rs
  - packages/web-ui/src/components/builds/mod.rs
  - packages/web-ui/assets/app.css
  - checks/web-ui/tests/integration-test.js
parent_task_id: TASK-347
priority: high
ordinal: 1751
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Child of Builds umbrella TASK-347. Follow guide doc-14 standard procedure.

## Problem
The Builds view (`views/builds.rs`) must match `CrystalForgelatest/components/BuildsView.jsx`. It currently has a mocked queue-build flow (line ~462: "Queue build flow is mocked in this UI pass").

## Goal
Pixel-align Builds queue/table + detail pane to the design and replace the mocked queue action with the real API (or remove the action if not yet supported, with a real disabled/explanatory state — not fake success).

## Exact scope
1. Active queue table density, columns, selection, first-row auto-select match design.
2. Detail pane (logs/metadata/actions) matches design.
3. Completed/history tab + filters match design.
4. Replace the mocked queue-build flow with a real API call OR a truthful disabled state; no fake "mocked in this UI pass" messaging in production.

## Non-goals
- Evaluations view (sibling TASK-345).
- Shared coherence-only changes covered by TASK-275 (coordinate, don't duplicate).

## Files
- packages/web-ui/src/views/builds.rs
- packages/web-ui/src/components/builds/**
- packages/web-ui/assets/app.css
- checks/web-ui/tests/integration-test.js

## Verification
- nix develop -c cargo fmt -- --check
- nix develop -c cargo check --manifest-path packages/web-ui/Cargo.toml --target wasm32-unknown-unknown
- nix build .#checks.x86_64-linux.web-ui
- Extend steps `15-builds`, `15d-builds-queue-table-view`, `15b-builds-completed-tab`, `15g-builds-action-visibility`.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Builds active queue/table density, columns, selection, and first-row auto-select match the design
- [ ] #2 Detail pane (logs/metadata/actions) matches the design
- [ ] #3 Completed/history tab + filters match the design
- [ ] #4 Mocked queue-build flow is replaced with a real API call or a truthful disabled state (no fake success/mock messaging)
- [ ] #5 web-ui steps screenshot queue/detail/completed and assert a real action
<!-- AC:END -->
