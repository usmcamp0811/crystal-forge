---
id: TASK-357
title: Close full Flakes surface parity gaps against CrystalForgelatest
status: In Progress
assignee:
  - gpt-5.5
created_date: '2026-06-14 18:56'
updated_date: '2026-06-15 01:21'
labels:
  - design-parity
  - flakes
  - web-ui
  - umbrella
milestone: 'm-19: Design Parity Existing Surfaces'
dependencies: []
priority: high
ordinal: 300000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem
The Flakes user experience (/flakes) has accumulated across multiple partial tasks and ad-hoc changes, but there is no single execution record that guarantees the entire Flakes surface matches the CrystalForgelatest design example end-to-end. Visual gaps, missing side-tray commit explorer, credential management, and inconsistent modal behavior can block acceptance.

## Goal
Bring the full Flakes surface into parity with the CrystalForgelatest reference across both `/flakes` list and the flake detail / commit tray surfaces, so a reviewer can compare the implemented UI against the design example and find no material visual or interaction discrepancies on the core desktop flows.

## Non-Goals
- Backend refactors unrelated to parity-driven API/data needs (flake CRUD, commit history endpoints, credential management API)
- Mobile-first redesign beyond responsive behavior already implied by the reference
- Replacing authoritative backend data with mock-only UI shortcuts in production paths, except temporary mock/placeholder data explicitly authorized for parity gaps without current backend support; any such mocks must be tracked by follow-up Backlog tasks

## Acceptance Criteria
- [ ] Flakes list header, stat strip (tracked, systems, synced count), filter/search bar, cards mode, and table mode materially match CrystalForgelatest on desktop
- [ ] Flakes list "Sync all" and "Add flake" header buttons behave and appear per the reference
- [ ] Flakes table mode columns (Flake, Status, Branch, Systems, Environments badges, Latest commit, Author, Synced, row actions) match the design layout, typography, and spacing
- [ ] Flakes cards mode (status rail, name/url, description, environments badges, KV stats, error callout, footer chips, Edit button) matches the card design
- [ ] Side tray (FlakeTray) commit explorer with timeline, pipeline dots, searchable commit list, commit detail pane (pipeline pills, rollout pill, files changed grid, DiffModal) materially matches the reference
- [ ] Add/Edit flake modal (name, URL, branch, environments display, description, credentials section with SSH/HTTPS pickers and add-new flow, sync section, danger zone) functionally matches the reference
- [ ] Delete flake confirmation with type-to-confirm guard matches the reference
- [ ] Loading, empty, error, and populated states are styled and behaved per the reference with no production-path mock fallback rendering (except authorized temporary placeholders)
- [ ] All displayed values are sourced from authoritative backend APIs in production paths unless explicitly tracked as backend follow-up gaps
- [ ] checks/web-ui captures screenshot evidence and behavior assertions for the full Flakes surface (/flakes)
- [ ] A human reviewer can compare the implemented Flakes surface against the CrystalForgelatest reference and find no remaining material parity gaps

## Architectural Constraints
- No business logic in UI views
- Existing repository patterns first (e.g., shared Icon component, Dioxus patterns)
- New flakes-related components go in packages/web-ui/src/components/flake/
- Side-tray commit explorer components in packages/web-ui/src/components/flake/ subdirectory
- Credential management follows existing API patterns; no secrets stored client-side
- Any temporary mock data must be clearly commented in code and tracked by a follow-up Backlog task

## Verification Plan
- cargo fmt -- --check
- cargo clippy -- -D warnings
- cargo test (targeted: flakes-related packages)
- Visual diff against the CrystalForgelatest reference for cards mode, table mode, side tray, and all modals
- checks/web-ui VM screenshot comparison (run on request)

## Impact Areas
- packages/web-ui/src/views/flakes_list.rs
- packages/web-ui/src/views/flakes.rs (flake detail)
- packages/web-ui/src/components/flake/ (various sub-components)
- packages/web-ui/assets/app.css
- checks/web-ui/default.nix (if screenshot coverage needs expansion)
- packages/default/src/handlers/api/flakes.rs (API changes if needed)
- packages/default/src/queries/flakes.rs (query changes if needed)

## Risk Level
Medium — primarily UI component work; backend changes minimal and scoped to parity data needs.

## Dependencies
- TASK-221 (flake credential management, already Done; should be re-evaluated for parity gaps)
- CrystalForgelatest reference: /home/mcamp/code/crystal-forge/CrystalForgelatest/components/FlakesView.jsx
<!-- SECTION:DESCRIPTION:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Audit current `/flakes` against `CrystalForgelatest/components/FlakesView.jsx`, focusing on already-existing `FlakesListViewNew`, `FlakeTrayNew`, Add/Edit modal, delete dialog, and web-ui screenshots.
2. Rework the list surface with minimal backend expansion only if required: add the missing 3-metric stat strip; align header action labels/casing and filter/table/card layout; ensure table and card cells render authoritative values or explicit unavailable/not persisted states.
3. Bring Add/Edit modal closer to reference without storing secrets client-side: keep existing credential API wiring; present credential mode controls and test/save behavior in the reference layout; clearly disable or mark unsupported auto-sync interval behavior if no backend persistence exists.
4. Replace the current delete dialog with a name-based type-to-confirm flow matching the reference while preserving backend deletion behavior where supported.
5. Tighten the side tray: verify commit list filtering, timeline buckets, selected commit detail, pipeline dots/pills, rollout pill, file grid, and diff modal against the reference; adjust labels/styling and add test affordances only where needed for screenshot checks.
6. Update `checks/web-ui/tests/integration-test.js` to assert and capture `/flakes` list/table parity, cards mode, Add/Edit modal credential surface, side tray commit explorer/diff modal, and delete type-to-confirm state.
7. Run targeted verification and update TASK-357 notes/acceptance criteria as each criterion is satisfied.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implementation plan approved by user and recorded before code changes.
<!-- SECTION:NOTES:END -->
