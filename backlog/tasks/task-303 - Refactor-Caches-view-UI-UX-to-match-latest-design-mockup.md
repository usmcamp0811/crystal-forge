---
id: TASK-303
title: Refactor Caches view UI/UX to match latest design mockup
status: In Progress
assignee: []
created_date: '2026-05-19 13:21'
updated_date: '2026-05-23 03:23'
labels:
  - ui
  - ux
  - caches
  - web-ui
  - design-system
milestone: UI/UX Design System
dependencies: []
references:
  - >-
    https://gitlab.com/crystal-forge/crystal-forge/-/blob/dev/packages/web-ui/src/views/caches.rs
  - /home/mcamp/code/crystal-forge/CrystalForgelatest/components/CachesView.jsx
modified_files:
  - packages/web-ui/src/views/caches.rs
  - checks/web-ui/tests/integration-test.js
  - packages/default/src/handlers/api/caches.rs
  - packages/web-ui/src/api/client.rs
  - packages/web-ui/src/api/models.rs
priority: medium
ordinal: 4800
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem
The current Caches view in the web UI is not aligned with the latest design interactions and layout patterns represented in the newest JSX mockup.

## Goal
Refactor the Caches view implementation so layout, information hierarchy, visual styling, interaction patterns, and responsive behavior align with the latest Caches design mockup.

## Design Source
- `/home/mcamp/code/crystal-forge/CrystalForgelatest/components/CachesView.jsx`

## Non-Goals
- No backend/API contract changes unless strictly required for UI parity and tracked separately.
- No unrelated redesign work outside Caches view surfaces.
- No broad shared-component library refactors unless directly required by Caches parity.

## Architectural Constraints
- Keep business logic out of view rendering code.
- Keep state/data mapping separated from presentational components.
- Reuse existing UI patterns/components where possible.
- Preserve DTO and endpoint compatibility unless explicitly approved in a separate task.

## Impact Areas
- `packages/web-ui/src/views/caches.rs`
- Caches-related components under `packages/web-ui/src/components/**` (if needed)
- `checks/web-ui/tests/integration-test.js` for Caches assertions/screenshots
- `packages/default/src/handlers/api/caches.rs` for credential-test hardening required by UI parity

## Verification Plan
- Compile verification:
  - `nix develop -c cargo check --manifest-path packages/web-ui/Cargo.toml --target wasm32-unknown-unknown`
  - `nix develop -c env SQLX_OFFLINE=true cargo check --manifest-path packages/default/Cargo.toml -p crystal-forge`
- Targeted checks:
  - `nix develop -c env SQLX_OFFLINE=true cargo test --manifest-path packages/default/Cargo.toml --lib handlers::api::caches`
  - `nix build .#checks.x86_64-linux.web-ui`
- Test evidence:
  - Update/add web-ui integration assertions for key Caches interactions from the mockup.
  - Capture updated screenshot evidence from web-ui checks and attach to MR.

## Risk Level
Medium: highly visible UI area; interaction/layout regressions are possible without targeted checks.

## Dependencies
- Design artifact remains authoritative and accessible:
  - `/home/mcamp/code/crystal-forge/CrystalForgelatest/components/CachesView.jsx`
- No conflicting active task modifying the same Caches files during execution.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Caches view layout and styling match the latest local design mockup across desktop and mobile breakpoints.
- [ ] #2 Primary Caches workflows and controls reflect the mockup's wording, hierarchy, and interaction behavior.
- [ ] #3 Any new/changed Caches-specific UI components keep presentation separated from state/data mapping.
- [ ] #4 No backend/API contract changes are introduced by this task except the reviewed and scoped `/api/v1/caches/test-credentials` credential-test response hardening required for UI parity.
- [ ] #5 `nix develop -c cargo check --manifest-path packages/web-ui/Cargo.toml --target wasm32-unknown-unknown` succeeds after implementation.
- [ ] #6 `nix build .#checks.x86_64-linux.web-ui` succeeds and includes Caches-view screenshot/assertion coverage proving intended behavior.
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
LOCK: gpt-5.3-codex on reckless in /home/mcamp/code/crystal-forge/TASK-303-refactor-caches-view
<!-- SECTION:NOTES:END -->
