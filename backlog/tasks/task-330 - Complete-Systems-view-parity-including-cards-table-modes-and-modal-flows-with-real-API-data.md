---
id: TASK-330
title: >-
  Complete Systems view parity including cards/table modes and modal flows with
  real API data
status: In Progress
assignee:
  - '@gpt-5.4'
created_date: '2026-05-31 15:56'
updated_date: '2026-06-13 14:36'
labels:
  - design-parity
  - systems
  - api-integration
milestone: m-19
dependencies:
  - TASK-328
  - TASK-329
references:
  - /home/mcamp/code/crystal-forge/CrystalForgelatest/app.jsx
modified_files:
  - checks/web-ui/tests/integration-test.js
  - packages/web-ui/src/components/dashboard/build_summary.rs
  - packages/web-ui/src/components/dashboard/cve_summary.rs
  - packages/web-ui/src/components/dashboard/mod.rs
  - packages/web-ui/src/components/system/edit_system_modal.rs
  - packages/web-ui/src/views/systems_list.rs
priority: high
ordinal: 1620
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Problem: Systems page is partially aligned but still drifts from design behavior/visuals and includes fallback/local-state patterns that can diverge from backend truth.

Goal: Achieve exact Systems view parity with CrystalForgelatest for both cards and table modes, with all displayed values sourced from real backend APIs.

Non-goals: New domain features not present in design examples.

Replan note: reset to Backlog as an m-19 vertical slice to be resumed after m-18 foundation tasks are complete enough to prevent rework.

Scope details:
- Align page header, stat strip, filter bar, cards/table geometry, chip styles, and selected-state behaviors.
- Match detail panel and modal visual/interaction behavior (deploy/edit/remove/update key).
- Remove/contain fallback/mock rendering in production path; ensure API-driven values for counts, statuses, and metadata.
- Ensure loading/empty/error states are styled per design and tied to real API outcomes.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Systems view card and table layouts are pixel-aligned with reference for core states
- [ ] #2 Filters/search/view toggles reproduce design behavior and result counts
- [ ] #3 All stat-strip and row/card values are sourced from backend APIs in production path
- [ ] #4 All systems modals/panels match reference spacing/typography/interactions
- [ ] #5 web-ui checks include screenshot + behavior assertions for systems parity scenarios
- [ ] #6 web-ui screenshot set covers loading, empty, error, and populated states for Systems surfaces
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Refresh the dedicated TASK-330 worktree onto the latest `dev` head before coding so the branch includes MR 272 and other merged work.
2. Phase 1: remove production-path systems mock/fallback rendering, replace it with real loading/empty/error states, and extend the existing systems API error check so it proves mock data never renders on API failure.
3. Open a draft MR for TASK-330 after Phase 1 verification so the user can review incremental progress without marking the task complete.
4. Phase 2: align the Systems header, stat strip, filter bar, cards/table density, selected-state treatment, and shown-count behavior to the CrystalForgelatest reference using real API-backed values.
5. Phase 3: align the side panel plus deploy/edit/add modal visuals and interactions, keeping API-backed submit flows and extending screenshot/assertion coverage for panel and modal states.
6. Run scoped verification after each phase and keep TASK-330 notes updated with progress, blockers, and MR context.
7. Review-fix pass for MR !273: drop unrelated backlog/task changes from the branch, make unsupported edit modal controls read-only or disabled, and ensure the danger-zone remove action is either wired to the existing remove flow or visibly non-actionable before rerunning targeted web-ui verification.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-06-13: Review-fix pass updated EditSystemModal so derived FQDN/branch fields are disabled display-only values, pinned commit UI is explicitly non-actionable until backend support exists, and the danger-zone action now routes into the existing Systems-list removal confirmation flow.

2026-06-13: Removed unrelated dashboard parity step from the systems integration test and restored dashboard widget component files back to dev so TASK-330 can ship without TASK-342.2 scope pollution.

2026-06-13 verification: `nix develop -c node --check checks/web-ui/tests/integration-test.js` passed; `nix develop -c cargo check --manifest-path packages/web-ui/Cargo.toml --target wasm32-unknown-unknown` passed; `nix develop -c cargo fmt --manifest-path packages/web-ui/Cargo.toml --all -- --check` ran successfully; `nix build .#checks.x86_64-linux.web-ui` was started and produced no error output before command completion, but I did not capture an explicit final success line beyond the build output stream.
<!-- SECTION:NOTES:END -->
