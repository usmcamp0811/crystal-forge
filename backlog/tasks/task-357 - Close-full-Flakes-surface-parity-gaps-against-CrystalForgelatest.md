---
id: TASK-357
title: Close full Flakes surface parity gaps against CrystalForgelatest
status: Review
assignee:
  - gpt-5.5
created_date: '2026-06-14 18:56'
updated_date: '2026-06-15 03:14'
labels:
  - design-parity
  - flakes
  - web-ui
  - umbrella
milestone: 'm-19: Design Parity Existing Surfaces'
dependencies: []
references:
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/277'
modified_files:
  - packages/web-ui/src/views/flakes_list.rs
  - checks/web-ui/tests/integration-test.js
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
<!-- AC:BEGIN -->
- [x] #1 Flakes list header, stat strip (tracked, systems, synced count), filter/search bar, cards mode, and table mode materially match CrystalForgelatest on desktop
- [x] #2 Flakes list "Sync all" and "Add flake" header buttons behave and appear per the reference
- [x] #3 Flakes table mode columns (Flake, Status, Branch, Systems, Environments badges, Latest commit, Author, Synced, row actions) match the design layout, typography, and spacing
- [x] #4 Flakes cards mode (status rail, name/url, description, environments badges, KV stats, error callout, footer chips, Edit button) matches the card design
- [x] #5 Side tray (FlakeTray) commit explorer with timeline, pipeline dots, searchable commit list, commit detail pane (pipeline pills, rollout pill, files changed grid, DiffModal) materially matches the reference
- [x] #6 Add/Edit flake modal (name, URL, branch, environments display, description, credentials section with SSH/HTTPS pickers and add-new flow, sync section, danger zone) functionally matches the reference
- [x] #7 Delete flake confirmation with type-to-confirm guard matches the reference
- [x] #8 Loading, empty, error, and populated states are styled and behaved per the reference with no production-path mock fallback rendering (except authorized temporary placeholders)
- [x] #9 All displayed values are sourced from authoritative backend APIs in production paths unless explicitly tracked as backend follow-up gaps
- [x] #10 checks/web-ui captures screenshot evidence and behavior assertions for the full Flakes surface (/flakes)
- [x] #11 A human reviewer can compare the implemented Flakes surface against the CrystalForgelatest reference and find no remaining material parity gaps

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

<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
MR !277 opened: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/277

Follow-up backend gaps created: TASK-357.1 for flake environment span API data and TASK-357.2 for flake auto-sync settings persistence.

Flakes web-ui check evidence produced: 13-flakes.png, 13a-flakes-cards-parity.png, 13aa-flakes-tray-diff-parity.png, 13e-flakes-add-modal-credentials.png, 13ea-flakes-delete-confirm-parity.png, 13f-flakes-edit-modal-credentials.png, 13g-flakes-edit-modal-ssh-save-persist.png.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Implemented Flakes registry parity improvements and opened MR !277.

Summary:
- Added Flakes stat strip and restored table/card environment badge surfaces.
- Stopped fabricating environment badges from build_scope; missing backend environment span data now renders as `not persisted` and is tracked by TASK-357.1.
- Added disabled/non-persisted auto-sync interval controls in Add/Edit modals and tracked backend persistence as TASK-357.2.
- Reworked delete confirmation into a reference-style type-the-flake-name flow.
- Expanded web-ui deterministic Flakes coverage for list, cards, tray/diff, add modal, delete confirmation, edit modal, and credential save/reopen screenshots.

Verification:
- `cargo check --manifest-path packages/web-ui/Cargo.toml --all-targets` — passed
- `cargo test --manifest-path packages/web-ui/Cargo.toml --bin crystal-forge-ui views::flakes_list::tests` — passed
- `cargo fmt --manifest-path packages/web-ui/Cargo.toml -- --check` — passed
- `node --check checks/web-ui/tests/integration-test.js` — passed
- `nix build .#checks.x86_64-linux.web-ui` — passed and produced Flakes screenshots
- `cargo clippy --manifest-path packages/web-ui/Cargo.toml --all-targets -- -D warnings` — run, but fails on pre-existing unrelated unused imports outside TASK-357 files

MR: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/277
<!-- SECTION:FINAL_SUMMARY:END -->
