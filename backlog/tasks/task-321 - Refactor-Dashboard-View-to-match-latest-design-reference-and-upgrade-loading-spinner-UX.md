---
id: TASK-321
title: >-
  Refactor Dashboard View to match latest design reference and upgrade loading
  spinner UX
status: In Progress
assignee: []
created_date: '2026-05-24 14:36'
updated_date: '2026-06-08 04:11'
labels:
  - ui
  - dashboard
  - loading
  - dioxus
milestone: 'm-6: UI Views - Dashboard'
dependencies: []
references:
  - >-
    /home/mcamp/code/crystal-forge/CrystalForgelatest/components/DashboardView.jsx
  - /home/mcamp/code/crystal-forge/CrystalForgelatest
modified_files:
  - packages/web-ui/src/views/dashboard.rs
  - checks/web-ui/tests/integration-test.js
priority: high
ordinal: 1000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

The current Crystal Forge dashboard view does not match the latest approved design reference in `CrystalForgelatest/components/DashboardView.jsx`, and the existing loading indicator UX is weaker than the newer spinner/loading treatment in `CrystalForgelatest`.

## Goal

Update the dashboard UI to align with the latest design reference and replace the current loading indicator with the improved spinner/loading experience from the design reference, delivered together in one scoped implementation.

## Non-Goals

- No backend/API contract changes unless strictly required for existing data rendering.
- No redesign of non-dashboard pages.
- No broad design system refactor outside components directly required for this dashboard update.

## Architectural Constraints

- Keep business logic out of view components; views should compose existing data/state and presentation components.
- Reuse existing UI patterns/components where possible; only introduce new shared UI pieces when they are dashboard-loading related and reusable.
- Preserve current dashboard data semantics while updating layout/presentation.

## Verification Plan

- nix develop -c cargo check --package default
- nix develop -c cargo test --package default web
- nix develop -c cargo test --package default handlers::api::dashboard
- Run/update web-ui check to capture dashboard screenshots, including visible loading spinner state

## Impact Areas

- Dashboard UI view/component files in `packages/default/src/ui/**`
- Any dashboard-specific loading component(s)
- Web UI screenshot/assertion checks for dashboard

## Risk Level

Medium

## Design References

- `/home/mcamp/code/crystal-forge/CrystalForgelatest/components/DashboardView.jsx`
- `/home/mcamp/code/crystal-forge/CrystalForgelatest` (loading spinner treatment)
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria

<!-- AC:BEGIN -->

- [ ] #1 Dashboard layout and visual structure are updated to match the latest design reference closely (component arrangement, hierarchy, and key styling intent).
- [x] #2 Dashboard loading indicator is replaced or upgraded to the improved spinner/loading treatment from the latest design reference.
- [x] #3 Loading state appears consistently in relevant dashboard data-fetch paths (initial load and refresh/loading transitions where applicable).
- [x] #4 Existing dashboard functionality/data rendering remains intact after visual and loading UX changes.
- [ ] #5 Web UI check captures updated dashboard visuals and includes evidence of the loading spinner state.
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-06-08: Continued TASK-321 in the dedicated worktree after reattaching the task branch and refreshing pre-flight proof.

Implemented remaining dashboard parity work in `packages/web-ui/src/views/dashboard.rs`: added local layout persistence via `StoredLayout`, edit-mode header controls (`Customize` / `Done`, reset layout), and an edit-mode guidance callout to better match the design reference while preserving the existing widget grid architecture.

Added `data-testid="dashboard-loading-spinner"` around the summary loading state so web-ui automation can capture visible loading evidence.

Updated `checks/web-ui/tests/integration-test.js` with a dedicated `06-dashboard-loading-spinner` step that delays dashboard summary + flake timeline responses, asserts the spinner/label are visible, and includes the screenshot step in `CI_FAST_STEP_NAMES`.

Verification run: `nix develop -c cargo check` from `packages/web-ui` passed; `nix build .#checks.x86_64-linux.web-ui` completed successfully.
<!-- SECTION:NOTES:END -->
