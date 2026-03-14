---
id: TASK-191
title: Replace blocking setup wizard with non-blocking guided coach panel
status: Review
assignee: []
created_date: '2026-03-14 13:17'
updated_date: '2026-03-14 21:04'
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

MR opened: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/165

Follow-up UX fix: coach Agent step now routes to Systems (instead of attempting in-place acknowledge), matching guided flow expectations.

Added systems-specific setup guidance callout explaining how to add a system and what agents are (`setup-coach-systems-callout`).

Updated web-ui integration test step `06f-onboarding-systems-callout` to assert the new systems guidance callout by test id.

Verification re-run: `nix develop -c cargo check` (packages/web-ui) passed.

Verification re-run: `nix build .#checks.x86_64-linux.web-ui -L` passed (37/37).

Pushed follow-up commit: 6a92a005 to MR !165.

Refined onboarding destination callouts to be tour-style/action-oriented (step labels + explicit action/why guidance), inspired by guided-tour UX rather than generic origin banners.

Removed redundant 'Back to Setup Coach' links from destination callouts since the coach panel remains visible and non-blocking.

Updated screenshot test assertions to use page-specific callout test IDs for deterministic validation.

Verification re-run: `nix develop -c cargo check` (packages/web-ui) passed.

Verification re-run: `nix build .#checks.x86_64-linux.web-ui -L` passed (37/37).

Pushed follow-up commit: 80fd09ea to MR !165.

Added JupyterLab-tour-style click-target callouts adjacent to primary onboarding actions (Add Environment, Add Flake, Add Builder, Add Destination, Add System).

Threaded onboarding hint state into Builders and Cache Destinations action areas so guidance appears directly on actionable controls.

Extended onboarding screenshot assertions to verify both page-level guidance callouts and click-target callouts are visible per step.

Verification re-run: `nix develop -c cargo check` (packages/web-ui) passed.

Verification re-run: `nix build .#checks.x86_64-linux.web-ui -L` passed (37/37).

Pushed follow-up commit: a0d34e8e to MR !165.

Refined click-target hints from chip-style labels to pointer callouts (bubble + anchor) for clearer guided-tour affordance.

Added subtle pulse/ring emphasis on guided action buttons while onboarding callouts are active.

Fixed cache-step behavior: Add Destination target callout now auto-hides once at least one cache destination exists, so guidance does not persist after completion.

Verification re-run: `nix develop -c cargo check` (packages/web-ui) passed.

Verification re-run: `nix build .#checks.x86_64-linux.web-ui -L` passed (37/37).

Pushed follow-up commit: 6833befc to MR !165.

Adjusted setup-progress cache completion semantics to treat any cache destination (including global/unassigned) as completing the cache onboarding step.

Updated `load_counts` in setup wizard handler to count `cache_destinations` directly instead of only env-assigned cache rows.

Added unit test `setup_progress_cache_step_accepts_global_destination` to lock behavior.

Verification: `nix develop -c cargo check` (packages/default) passed.

Verification: `nix develop -c cargo test setup_progress_cache_step_accepts_global_destination` (packages/default) passed.

Verification: `nix build .#checks.x86_64-linux.web-ui -L` passed (37/37).

Pushed follow-up commit: 41e5f78a to MR !165.

Fixed systems page target callout layering so it renders above health/deployment filter dropdown overlays.

Applied stacking-context update in `packages/web-ui/src/views/systems_list.rs` (`relative z-40` container + callout `z-index:70`).

Verification: `nix develop -c cargo check` (packages/web-ui) passed.

Verification: `nix build .#checks.x86_64-linux.web-ui -L` passed (37/37).

Pushed follow-up commit: 6cefdb1c to MR !165.

Added post-create onboarding reminder on Systems page when creation happens in setup-coach context.

Reminder explicitly instructs admins to enable the Crystal Forge agent module in host config, apply/rebuild host config, and ensure agent service is running before expecting telemetry/deploy status.

Reminder rendered as non-blocking banner (`setup-coach-agent-runtime-reminder`) and scoped to onboarding flow only.

Verification: `nix develop -c cargo check` (packages/web-ui) passed.

Verification: `nix build .#checks.x86_64-linux.web-ui -L` passed (37/37).

Pushed follow-up commit: 52b6477e to MR !165.

Promoted post-system onboarding reminder to high-visibility modal popup (`setup-coach-agent-runtime-reminder-modal`) with explicit dismissal action.

Modal is gated to setup-coach context and only triggers when creating the first system in that onboarding run (`local_systems` empty before create).

Reminder content continues to explicitly require agent module enablement, host config apply/rebuild, and running agent service before tracking appears.

Verification: `nix develop -c cargo check` (packages/web-ui) passed.

Verification: `nix build .#checks.x86_64-linux.web-ui -L` passed (37/37).

Pushed follow-up commit: 91081810 to MR !165.

Fixed onboarding expectation gap: after first system creation in setup-coach flow, the UI now calls `set_setup_wizard_agent_acknowledged(true)` so Deploy Agent step turns complete/green automatically.

Removed redundant coach footer action button (`Mark agent understood`) now that modal reminder + automatic acknowledgment handle this state transition.

Updated agent-step helper text in coach panel to indicate automatic completion after first system setup.

Verification: `nix develop -c cargo check` (packages/web-ui) passed.

Verification: `nix build .#checks.x86_64-linux.web-ui -L` passed (37/37).

Pushed follow-up commit: 4b4706c0 to MR !165.

Unified onboarding callout styling to match the agent activation modal color system (blue background/border/text palette) across environments, flakes, builders, caches, and systems pages.

Updated both page-level guidance callouts and click-target pointer bubbles; aligned pulse emphasis rings from violet to blue for visual consistency.

No selector/test-id changes were made, preserving screenshot/test determinism.

Verification: `nix develop -c cargo check` (packages/web-ui) passed.

Verification: `nix build .#checks.x86_64-linux.web-ui -L` passed (37/37).

Pushed follow-up commit: 8d6ff3be to MR !165.

Added field-level onboarding callouts in Add System, Add Flake, and Add Builder forms; each callout now auto-dismisses when the user focuses/edits that field.

Raised flakes header target-callout layering (`z-[2101]` container + callout `z-index:2200`) so it stays above commit/size filter dropdown overlays.

Added first-builder onboarding activation modal (`setup-coach-builder-runtime-reminder-modal`) mirroring system reminder semantics, shown only in setup-coach flow when creating the first builder.

Verification: `nix develop -c cargo check` (packages/web-ui) passed.

Verification: `nix build .#checks.x86_64-linux.web-ui -L` passed (37/37).

Pushed follow-up commit: 987b056a to MR !165.

2026-03-14: Added onboarding guidance inside Add Cache Destination modal with field-level callouts for name, endpoint, and environments, each auto-dismissing on interaction/input. Added explicit resource/concurrency warning block in Add Builder modal to prevent overload misconfiguration during first-time setup.

Verification:
- nix develop -c cargo check (packages/web-ui)
- nix build .#checks.x86_64-linux.web-ui -L (37/37)

Commit: b2eec0a1
MR: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/165

2026-03-14: Added first-run explanatory callout in Create Environment -> Required Policies section to clarify that policies are hard configuration-level requirements for deployments, and that required policies can be adjusted per environment later.

Verification:
- nix develop -c cargo check (packages/web-ui)
- nix build .#checks.x86_64-linux.web-ui -L (37/37)

Commit: 00c0e606
MR: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/165

2026-03-14: Refined Create Environment required-policies helper styling/copy based on UX feedback: replaced heavy blue callout with neutral inline helper note and rewrote content in plain-English, beginner-focused language.

Verification:
- nix develop -c cargo check (packages/web-ui)
- nix build .#checks.x86_64-linux.web-ui -L (37/37)

Commit: e90781e5
MR: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/165

2026-03-14: Updated Create Environment required-policies helper styling to match the established blue onboarding callout theme across onboarding surfaces, while preserving the beginner-friendly plain-English content.

Verification:
- nix develop -c cargo check (packages/web-ui)
- nix build .#checks.x86_64-linux.web-ui -L (37/37)

Commit: d8f87f4f
MR: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/165

2026-03-14: Addressed UX feedback that Required Policies text blended in. Reworked Create Environment guidance into an explicit onboarding callout container with strong blue themed styling, a clear callout heading, and structured beginner-focused lines.

Verification:
- nix develop -c cargo check (packages/web-ui)
- nix build .#checks.x86_64-linux.web-ui -L (37/37)

Commit: 053b3a2e
MR: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/165

2026-03-14: Applied broader UX request to make onboarding helper text unmistakable callouts across setup forms. Converted field guidance in builder/system/flake/cache forms into explicit high-contrast setup-coach callout containers (heading + emphasized callout block), and upgraded builder Resource guidance block to the same callout treatment. Also fixed attribute order in environment policy callout block.

Verification:
- nix develop -c cargo check (packages/web-ui)
- nix build .#checks.x86_64-linux.web-ui -L (37/37)

Commit: 4f16cadd
MR: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/165

2026-03-14: Addressed UX regression report on callout inconsistency. Normalized onboarding helper callouts across Create Environment, Add Builder, Add System, Register Flake, and Add Cache Destination to match the proven "Next action" callout style used near Add Environment (same visual weight, heading, spacing, border, and shadow). Removed over-styled variants introduced in prior pass.

Verification:
- nix develop -c cargo check (packages/web-ui)
- nix build .#checks.x86_64-linux.web-ui -L (37/37)

Commit: bc68cb2b
MR: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/165

2026-03-14: Applied explicit callout-anchor pass after UX feedback that form hints still read as cards. Added pointer-notch anchor cues to onboarding form hints while preserving the established Next action callout palette/typography, so each hint now visually reads as a callout rather than a generic bubble/card.

Verification:
- nix develop -c cargo check (packages/web-ui)
- nix build .#checks.x86_64-linux.web-ui -L (37/37)

Commit: ab7d3517
MR: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/165
<!-- SECTION:NOTES:END -->
