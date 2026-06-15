---
id: TASK-357
title: Close full Flakes surface parity gaps against CrystalForgelatest
status: In Progress
assignee:
  - gpt-5.5
created_date: '2026-06-14 18:56'
updated_date: '2026-06-15 01:18'
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

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
LOCK: gpt-5.5 on reckless in /home/mcamp/code/crystal-forge/TASK-357-flakes-surface-parity
<!-- SECTION:NOTES:END -->
