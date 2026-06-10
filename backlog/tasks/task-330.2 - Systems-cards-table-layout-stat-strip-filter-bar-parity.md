---
id: TASK-330.2
title: 'Systems: cards/table layout + stat strip + filter bar parity'
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
  - /home/mcamp/code/crystal-forge/CrystalForgelatest/components/Systems.jsx
  - design/doc-8 - CrystalForgelatest-UI-Parity-Matrix-TASK-328.md
  - design/doc-14 - Parity-execution-playbook-agent-proof.md
documentation:
  - design/doc-8 - CrystalForgelatest-UI-Parity-Matrix-TASK-328.md
  - design/doc-14 - Parity-execution-playbook-agent-proof.md
modified_files:
  - packages/web-ui/src/views/systems_list.rs
  - packages/web-ui/src/components/system/system_card_v2.rs
  - packages/web-ui/src/components/tables/systems_table.rs
  - packages/web-ui/src/components/systems_stat_strip.rs
  - packages/web-ui/assets/app.css
  - checks/web-ui/tests/integration-test.js
parent_task_id: TASK-330
priority: high
ordinal: 1622
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Child of Systems umbrella TASK-330. Follow guide doc-14 standard procedure.

## Problem
The Systems page header, stat strip, filter bar, and cards/table geometry must match `CrystalForgelatest/app.jsx` (SystemsView) and `components/Systems.jsx` exactly.

## Goal
Pixel-align the Systems list shell: page head, stat strip, filter bar, view toggle, and both cards and table density, using values from doc-8 tolerances.

## Exact scope (match design)
1. Page head: title + subtitle counts ("N systems · N healthy · N needing attention") + Export/Add buttons.
2. Stat strip: Total / Healthy / Warning-drift / Critical-offline / CVEs(critical) tiles with spark/segments as in design.
3. Filter bar: search input + environment/status/flake/tag selects + cards|table segmented toggle + "N shown" count.
4. Cards grid and table density match design row/card rhythm, chips, and selected state.
5. All counts/values come from the real API data already loaded by the view (no new mock).

## Non-goals
- Modal/side-panel parity (separate sibling task).
- Removing fallback data (separate sibling task: "remove mock/fallback").

## Files
- packages/web-ui/src/views/systems_list.rs
- packages/web-ui/src/components/system/** (system_card_v2.rs, cards.rs)
- packages/web-ui/src/components/tables/systems_table.rs
- packages/web-ui/src/components/systems_stat_strip.rs
- packages/web-ui/assets/app.css
- checks/web-ui/tests/integration-test.js

## Verification
- nix develop -c cargo fmt -- --check
- nix develop -c cargo check --manifest-path packages/web-ui/Cargo.toml --target wasm32-unknown-unknown
- nix build .#checks.x86_64-linux.web-ui
- Extend step `12-systems` to screenshot cards + table modes and assert the view toggle and "N shown" count update.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Page head, stat strip, and filter bar match the design layout within doc-8 tolerances
- [ ] #2 Cards and table modes both match design density, chips, and selected state
- [ ] #3 View toggle switches modes and the 'N shown' count reflects active filters
- [ ] #4 Stat strip values are derived from the real loaded API data
- [ ] #5 web-ui step screenshots cards and table modes and asserts the toggle + shown count
<!-- AC:END -->
