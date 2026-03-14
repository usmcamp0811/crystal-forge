---
id: TASK-191
title: Replace blocking setup wizard with non-blocking guided coach panel
status: In Progress
assignee: []
created_date: '2026-03-14 13:17'
updated_date: '2026-03-14 13:56'
labels:
  - frontend
  - ux
  - onboarding
  - admin
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

The current full-page setup wizard is useful but blocks normal navigation and feels heavy. Users want onboarding that teaches the product in-context while still letting them use the app immediately.

## Goal

Implement a non-blocking guided onboarding experience using a floating coach panel (top-left or top-right) that:
- shows setup checklist progress
- lets users jump directly to related views
- provides contextual callouts/tooltips on relevant pages
- can be minimized/dismissed and resumed later

## Non-Goals

- Do not redesign existing entity CRUD pages (builders/caches/systems/etc.)
- Do not replace backend setup progress semantics already implemented in TASK-187
- Do not add analytics/telemetry collection in this task
- Do not require first-time users to complete onboarding before using the app

## Proposed UX

1. Floating coach panel available after first admin login (and re-openable from admin)
2. Checklist of steps: Environment, Flake, Builder, Cache, System, Agent Acknowledgment
3. Clicking a step routes to the relevant page and keeps coach context alive
4. Destination pages show contextual callouts for key actions (e.g., Add Builder, Add Cache)
5. Coach panel displays completion status from backend progress endpoint
6. Panel supports minimize, dismiss, and resume

## Architecture Constraints

- Reuse existing setup progress APIs from TASK-187 (no duplicate progress logic)
- Keep UI guidance state in web-ui layer (no business logic in view rendering)
- Avoid hidden global mutable state; prefer explicit signal/context for coach session state
- Keep route-level behavior backward compatible and admin-gated

## Impact Areas

- packages/web-ui/src/views/setup.rs (decompose/reuse setup step definitions as needed)
- packages/web-ui/src/components/notifications/ or new components/onboarding/
- packages/web-ui/src/views/{builders,environments_list,caches,flakes_list,systems_list}.rs
- packages/web-ui/src/views/admin.rs (entry point to relaunch onboarding)
- packages/web-ui/src/routes.rs (if coach route/state hooks are needed)
- optional backend touch only if API contract changes are required (prefer none)

## Risk Level

Medium — mostly UX orchestration and state consistency across route changes.

## Verification Plan

Tier 0:
- nix develop -c cargo check (web-ui)
- rustfmt --check for touched files
- targeted tests for any new helper/state logic

Tier 1 manual:
- First admin sees floating coach panel (not blocking full page)
- Can navigate app while coach remains available
- Clicking each checklist item routes correctly
- Contextual callouts appear on matching page and can be dismissed
- Progress updates when entities are created outside the coach flow
- Dismiss/minimize/resume behavior works consistently
- Non-admin users do not see coach panel
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A floating non-blocking coach panel appears for first-time admin onboarding instead of forcing a full-page wizard.
- [ ] #2 Coach panel shows all onboarding steps with live completion state from setup-progress API.
- [ ] #3 Clicking a coach step routes to the corresponding view and preserves onboarding context.
- [ ] #4 Each target view includes at least one contextual callout guiding the key setup action.
- [ ] #5 Users can minimize and dismiss the coach panel; dismissed state persists per existing wizard-state semantics.
- [ ] #6 Users can relaunch onboarding from Server Management.
- [ ] #7 Non-admin users cannot access onboarding coach UI.
- [ ] #8 Manual onboarding flow can be completed without being blocked from normal app navigation.
- [ ] #9 Existing setup progress and dismissal APIs remain backward compatible (or are updated with tests/docs if changed).
- [ ] #10 Web-ui compile and formatting checks pass for touched files.
- [ ] #11 Web UI checks include deterministic screenshot coverage for the full onboarding coach flow (entry, each checklist step context, completion state).
- [ ] #12 The existing web-ui screenshot check pipeline is updated to capture onboarding coach-panel states and linked page callouts in sequence.
- [ ] #13 Generated screenshots are attached to the merge request (via GitLab uploads) and referenced in the MR UI Changes section.
- [ ] #14 Screenshot capture is reproducible in local/Nix CI runs and fails the check when expected onboarding screenshots are missing.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Add/extend the `checks/web-ui` flow to script the complete onboarding sequence and save labeled screenshots for each phase.

Store expected screenshot artifacts under the existing web-ui check conventions and enforce existence/consistency in the check result.

Update MR workflow notes for this task: include all onboarding screenshots in the MR description using GitLab upload links.

Validate end-to-end locally by running the updated web-ui check and confirming artifact outputs before opening MR.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Moved to To Do per maintainer request for near-term follow-up after MR !164 merge.

Maintainer request: TASK-191 must include full setup-process screenshots, with web-ui check automation updated accordingly; screenshots are required MR evidence for this task.

LOCK: OpenCode on reckless in ~/code/crystal-forge/TASK-191-non-blocking-onboarding-coach-panel

Implemented non-blocking onboarding coach panel in web-ui and integrated it into app shell for admins.

Replaced forced /setup redirects in login/register flows with normal app navigation while retaining onboarding context via coach step navigation + callout banners.

Updated checks/web-ui integration flow to capture deterministic onboarding screenshots (06a-06f) and added required screenshot enforcement in checks/web-ui/default.nix.

Fixed mobile sidebar screenshot flake by making coach panel responsive on small screens (bottom anchored + viewport-constrained width) so it no longer intercepts mobile nav toggle clicks.

Verification: `nix develop -c cargo check` (packages/web-ui) passed.

Verification: `nix develop -c cargo fmt -- --check` (packages/web-ui) passed.

Verification: `nix build .#checks.x86_64-linux.web-ui -L` passed with 37/37 screenshot steps including `09c-sidebar-mobile-drawer`.

Pushed branch: TASK-191-non-blocking-onboarding-coach-panel (commit e7680a45).
<!-- SECTION:NOTES:END -->
