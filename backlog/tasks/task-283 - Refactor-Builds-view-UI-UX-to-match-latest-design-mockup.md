---
id: TASK-283
title: Refactor Builds view UI/UX to match latest design mockup
status: Backlog
assignee: []
created_date: '2026-04-30 21:32'
labels:
  - ui
  - ux
  - builds
  - web-ui
  - design-system
milestone: UI/UX Design System
dependencies: []
references:
  - /home/mcamp/code/crystal-forge/crystal-forge/project/data-builds.js
priority: medium
ordinal: 4500
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem
The current Builds view does not fully match the latest UI/UX design direction and interaction model represented by the newest mockup artifacts.

## Goal
Refactor the Builds view implementation so layout, information hierarchy, visual styling, interaction patterns, and responsive behavior align with the latest Builds design mockup.

## Design Source
- Local mockup/data reference: `/home/mcamp/code/crystal-forge/crystal-forge/project/data-builds.js`
- Any companion local design assets in `/home/mcamp/code/crystal-forge/crystal-forge` that define the latest Builds view should be treated as authoritative for this task.

## Non-Goals
- No backend/API contract changes unless strictly required for UI parity.
- No unrelated redesign work outside Builds view surfaces.
- No broad refactors of shared component libraries unless directly required by Builds view requirements.

## Architectural Constraints
- Keep business logic out of views.
- Keep state/adapter logic separated from presentational components.
- Reuse existing UI patterns/components where possible; create new reusable components only when necessary for design parity.
- Preserve DTO compatibility unless explicit API change is approved in a separate task.

## Impact Areas
- `packages/web-ui/src/views/builds.rs`
- Builds-related components under `packages/web-ui/src/components/**`
- Builds adapter/state mapping files as needed
- UI test/checks in `checks/web-ui/tests/**` for Builds view assertions/screenshots

## Verification Plan
- Compile verification:
  - `nix develop -c cargo check` (web-ui crate)
- Targeted UI check verification:
  - `nix build .#checks.x86_64-linux.web-ui`
- Test evidence:
  - Update/add web-ui integration check steps that assert key Builds UI behavior from the new design.
  - Capture updated screenshot evidence from `web-ui` checks and attach to MR.
- Regression verification:
  - Ensure existing Builds interactions (queue/history display, status badges, action affordances) still function as expected.

## Risk Level
Medium: high visible surface area and potential interaction regressions if styling/layout and behavior updates are not aligned.

## Dependencies
- Latest design mockup artifacts in `/home/mcamp/code/crystal-forge/crystal-forge` must be accessible and unambiguous.
- No active conflicting UI redesign task modifying the same Builds view files during execution.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Builds view visual layout and styling match the latest local design mockup for desktop and mobile breakpoints.
- [ ] #2 Build queue, active jobs, history, worker/status sections, and key badges/controls reflect the mockup’s information hierarchy and wording.
- [ ] #3 Any new/changed Builds-specific components are reusable and keep presentation separated from state/data mapping.
- [ ] #4 No backend/API contract is changed as part of this task (unless separately approved and tracked).
- [ ] #5 `nix develop -c cargo check` (web-ui) succeeds after implementation.
- [ ] #6 `nix build .#checks.x86_64-linux.web-ui` succeeds and includes updated Builds-view screenshot/assertion coverage proving intended UI behavior.
<!-- AC:END -->
