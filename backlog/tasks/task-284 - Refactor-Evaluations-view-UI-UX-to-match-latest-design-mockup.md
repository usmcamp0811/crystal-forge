---
id: TASK-284
title: Refactor Evaluations view UI/UX to match latest design mockup
status: Review
assignee: []
created_date: '2026-04-30 21:32'
updated_date: '2026-05-24 01:17'
labels:
  - ui
  - ux
  - evaluations
  - web-ui
  - design-system
milestone: UI/UX Design System
dependencies: []
references:
  - /home/mcamp/code/crystal-forge/crystal-forge/project/data-builds.js
priority: medium
ordinal: 4600
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem
The current Evaluations view does not fully match the latest UI/UX design direction and interaction model represented by the newest mockup artifacts.

## Goal
Refactor the Evaluations view implementation so layout, information hierarchy, visual styling, interaction patterns, and responsive behavior align with the latest Evaluations design mockup.

## Design Source
- Local mockup/data reference: `/home/mcamp/code/crystal-forge/crystal-forge/project/data-builds.js` (Evaluations section)
- Any companion local design assets in `/home/mcamp/code/crystal-forge/crystal-forge` that define the latest Evaluations view should be treated as authoritative for this task.

## Non-Goals
- No backend/API contract changes unless strictly required for UI parity.
- No unrelated redesign work outside Evaluations view surfaces.
- No broad refactors of shared component libraries unless directly required by Evaluations view requirements.

## Architectural Constraints
- Keep business logic out of views.
- Keep state/adapter logic separated from presentational components.
- Reuse existing UI patterns/components where possible; create new reusable components only when necessary for design parity.
- Preserve DTO compatibility unless explicit API change is approved in a separate task.

## Impact Areas
- `packages/web-ui/src/views/evaluations.rs`
- Evaluations-related components under `packages/web-ui/src/components/**`
- Evaluations adapter/state mapping files as needed
- UI test/checks in `checks/web-ui/tests/**` for Evaluations view assertions/screenshots

## Verification Plan
- Compile verification:
  - `nix develop -c cargo check` (web-ui crate)
- Targeted UI check verification:
  - `nix build .#checks.x86_64-linux.web-ui`
- Test evidence:
  - Update/add web-ui integration check steps that assert key Evaluations UI behavior from the new design.
  - Capture updated screenshot evidence from `web-ui` checks and attach to MR.
- Regression verification:
  - Ensure existing Evaluations interactions (active queue, status transitions, cancel/force-cancel affordances, history display) still function as expected.

## Risk Level
Medium: high visible surface area and potential interaction regressions if styling/layout and behavior updates are not aligned.

## Dependencies
- Latest design mockup artifacts in `/home/mcamp/code/crystal-forge/crystal-forge` must be accessible and unambiguous.
- No active conflicting UI redesign task modifying the same Evaluations view files during execution.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Evaluations view visual layout and styling match the latest local design mockup for desktop and mobile breakpoints.
- [ ] #2 Active evaluations queue, history, statuses, and action controls reflect the mockup’s information hierarchy and wording.
- [ ] #3 Any new/changed Evaluations-specific components are reusable and keep presentation separated from state/data mapping.
- [ ] #4 No backend/API contract is changed as part of this task (unless separately approved and tracked).
- [ ] #5 `nix develop -c cargo check` (web-ui) succeeds after implementation.
- [ ] #6 `nix build .#checks.x86_64-linux.web-ui` succeeds and includes updated Evaluations-view screenshot/assertion coverage proving intended UI behavior.
<!-- AC:END -->
