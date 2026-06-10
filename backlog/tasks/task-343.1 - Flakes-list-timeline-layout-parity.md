---
id: TASK-343.1
title: 'Flakes: list/timeline layout parity'
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
  - packages/web-ui/src/components/flake/flake_timeline.rs
  - packages/web-ui/assets/app.css
  - checks/web-ui/tests/integration-test.js
parent_task_id: TASK-343
priority: high
ordinal: 1741
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Child of Flakes umbrella TASK-343. Follow guide doc-14 standard procedure.

## Problem
The Flakes view (`packages/web-ui/src/views/flakes.rs`) must match `CrystalForgelatest/components/FlakesView.jsx` for list/timeline layout, chips, and density.

## Goal
Pixel-align the Flakes list and commit timeline to the design within doc-8 tolerances, backed by real API data.

## Exact scope
1. Page head + filter/search controls match design.
2. Flake list rows + commit timeline spacing, chips, and status indicators match design.
3. Evaluation status chip semantics match design (complete/partial/error) using real data.
4. All values from the real API (no fabricated data).

## Non-goals
- Legacy `flakes_list.rs` removal (tracked by TASK-297.1 / TASK-341).
- Modal flows (sibling task TASK-343.x if needed).

## Files
- packages/web-ui/src/views/flakes.rs
- packages/web-ui/src/components/flake/flake_timeline.rs
- packages/web-ui/assets/app.css
- checks/web-ui/tests/integration-test.js

## Verification
- nix develop -c cargo fmt -- --check
- nix develop -c cargo check --manifest-path packages/web-ui/Cargo.toml --target wasm32-unknown-unknown
- nix build .#checks.x86_64-linux.web-ui
- Extend step `13-flakes` to screenshot list+timeline and assert filter/search behavior.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Flakes page head, filter/search, list rows, and commit timeline match the design within doc-8 tolerances
- [ ] #2 Evaluation status chip semantics match the design using real data
- [ ] #3 No fabricated values render in the production path
- [ ] #4 web-ui step screenshots the Flakes list+timeline and asserts filter/search behavior
<!-- AC:END -->
