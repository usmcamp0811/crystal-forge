---
id: TASK-358
title: Close full Environments surface parity gaps against CrystalForgelatest
status: Review
assignee: []
created_date: '2026-06-14 18:56'
updated_date: '2026-06-15 00:54'
labels:
  - design-parity
  - environments
  - web-ui
  - umbrella
milestone: 'm-19: Design Parity Existing Surfaces'
dependencies: []
references:
  - >-
    /home/mcamp/code/crystal-forge/CrystalForgelatest/components/EnvironmentsView.jsx
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/276'
documentation:
  - >-
    /home/mcamp/code/crystal-forge/CrystalForgelatest/components/EnvironmentsView.jsx
modified_files:
  - packages/default/migrations/0139_add_view_environment_rollups.sql
  - packages/default/src/api/models.rs
  - packages/default/src/queries/environments.rs
  - packages/default/src/handlers/api/environments.rs
  - packages/web-ui/src/views/environments_list.rs
  - packages/web-ui/src/components/environments/environment_card.rs
  - packages/web-ui/src/components/environments/environment_form_modal.rs
  - packages/web-ui/src/components/environments/remove_environment_dialog.rs
  - packages/web-ui/src/components/environments/mod.rs
  - packages/web-ui/src/environments/adapter.rs
  - packages/web-ui/src/api/models.rs
  - packages/web-ui/assets/app.css
  - checks/web-ui/tests/integration-test.js
priority: high
ordinal: 301000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem
The Environments user experience (/environments) has accumulated across partial tasks and ad-hoc changes, but there is no single execution record that guarantees the entire Environments surface matches the CrystalForgelatest design example end-to-end. Visual gaps, inconsistent stat strip, health bars, cache assignment, policy enforcement UI, and modal behavior can block acceptance.

## Goal
Bring the full Environments surface into parity with the CrystalForgelatest reference at `/environments`, so a reviewer can compare the implemented UI against the design example and find no material visual or interaction discrepancies on the core desktop flows.

## Non-Goals
- Environments sidebar surface (tracked separately by TASK-339)
- Backend refactors unrelated to parity-driven API/data needs (environment CRUD, cache assignment API, compliance bundles, gate policies)
- Mobile-first redesign beyond responsive behavior already implied by the reference
- Replacing authoritative backend data with mock-only UI shortcuts in production paths, except temporary mock/placeholder data explicitly authorized for parity gaps without current backend support; any such mocks must be tracked by follow-up Backlog tasks

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Environments list header, subtitle (tiers, systems, caches count), and "Add environment" button materially match CrystalForgelatest on desktop
- [x] #2 Stat strip displays 5 metrics (Total tiers, Systems, Caches, Manual policy, Auto-sync off) with colored accent rails per the reference
- [x] #3 Filter bar with search, cards/table view toggle, and count text matches the reference
- [x] #4 Cards mode: env card with color rail, name/title with PROD badge, description, systems stat, flake chips, health bar (colored segments), health legend (with CVE count), KV grid (Deploy, Enforcement, Cache, Auto-sync, Approval), footer (role assignments, Edit button) — all materially match the reference
- [x] #5 Table mode columns (Environment, Systems, Health bar, Deploy, Enforcement, Cache, Auto-sync, Approval, row actions) match the design layout, typography, and spacing
- [x] #6 Add/Edit environment modal (name, color picker with presets and custom, description, cache assignment with dropdown and detail display, deployment mode selector, gate policy picker with search/multi-select, compliance bundle selector, production toggle, auto-sync/approval toggles, danger zone) functionally matches the reference
- [x] #7 Delete environment confirmation with type-to-confirm and systems guard matches the reference
- [x] #8 Loading, empty, error, and populated states are styled and behaved per the reference with no production-path mock fallback rendering (except authorized temporary placeholders)
- [x] #9 All displayed values are sourced from authoritative backend APIs in production paths unless explicitly tracked as backend follow-up gaps
- [x] #10 checks/web-ui captures screenshot evidence and behavior assertions for the full Environments surface (/environments)
- [x] #11 A human reviewer can compare the implemented Environments surface against the CrystalForgelatest reference and find no remaining material parity gaps

## Architectural Constraints
- No business logic in UI views
- Existing repository patterns first (e.g., shared Icon component, Dioxus patterns)
- New environments-related components go in packages/web-ui/src/components/environments/
- Color picker uses native HTML input[type=color] per the reference; present palette is a shortcut
- Cache assignment dropdown references cache destinations from the Caches view (may need cross-view data wiring)
- Gate policy picker and compliance bundle selector reference the Policies and Compliance views data (may need API endpoints)
- Any temporary mock data must be clearly commented in code and tracked by a follow-up Backlog task

## Verification Plan
- cargo fmt -- --check
- cargo clippy -- -D warnings
- cargo test (targeted: environments-related packages)
- Visual diff against the CrystalForgelatest reference for cards mode, table mode, and all modals
- checks/web-ui VM screenshot comparison (run on request)

## Impact Areas
- packages/web-ui/src/views/environments_list.rs
- packages/web-ui/src/views/environments.rs (env detail)
- packages/web-ui/src/components/environments/ (new sub-components)
- packages/web-ui/assets/app.css
- checks/web-ui/default.nix (if screenshot coverage needs expansion)
- packages/default/src/handlers/api/environments.rs (API changes if needed)
- packages/default/src/queries/environments.rs (query changes if needed)

## Risk Level
Medium — primarily UI component work; backend changes moderate if cache/policy/compliance data wiring is missing; overlaps with TASK-339 for sidebar surface (separate tracking).

## Dependencies
- TASK-339 (Environments sidebar surface umbrella — separate scope, but may share components)
- CrystalForgelatest reference: /home/mcamp/code/crystal-forge/CrystalForgelatest/components/EnvironmentsView.jsx
<!-- SECTION:DESCRIPTION:END -->

<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
MR !276 migration repair note added: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/276#note_3454176150
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Migration repair addressed in 16a90840: restored 0139 to its original contents and added 0140_update_environment_rollups_active_system_count.sql for the forward-only view change. Applied 0140 locally and reran SQLx/backend/web-ui targeted verification.
<!-- SECTION:FINAL_SUMMARY:END -->
