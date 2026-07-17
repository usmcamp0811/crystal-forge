---
id: TASK-394
title: >-
  Implement the 2026-07-16 design delta for cross-surface navigation, focus-prop
  system, edit-system tabbed modal, and cache-push detail parity
status: Backlog
assignee: []
created_date: '2026-07-16 00:00'
updated_date: '2026-07-16 00:00'
labels:
  - design-parity
  - web-ui
  - cross-surface
  - backlog-capture
dependencies:
  - TASK-393
references:
  - commit e0c7b724 (`Design update`)
  - docs/design/CrystalForge/app.jsx
  - docs/design/CrystalForge/components/BuildsView.jsx
  - docs/design/CrystalForge/components/DashboardView.jsx
  - docs/design/CrystalForge/components/EditSystemModal.jsx
  - docs/design/CrystalForge/components/EvalDrawer.jsx
  - docs/design/CrystalForge/components/EvalsView.jsx
  - docs/design/CrystalForge/components/FlakesView.jsx
  - docs/design/CrystalForge/components/PoliciesView.jsx
  - docs/design/CrystalForge/components/SystemDetail.jsx
  - docs/design/CrystalForge/data-builds.js
  - docs/design/CrystalForge/fixtures/crystal-forge.fixtures.js
  - docs/design/CrystalForge/fixtures/crystal-forge.fixtures.json
  - backlog/docs/specs/doc-19 - Spec-All-views-visual-drift-audit-against-updated-design-example.md
documentation:
  - backlog/milestones/m-19 - design-parity-existing-surfaces.md
priority: high
ordinal: 394000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem Statement

The design example was updated again in commit `e0c7b724` (`Design update`) on
2026-07-16, on top of the earlier builders delta (`2cc51a2d`). This update
introduces a cross-cutting interaction pattern not yet present in the shipped
UI: a focus-prop system that allows views to deep-link into one another (e.g.,
Dashboard git-graph failure badges navigate to Builds or Evals filtered to a
specific commit; Flakes-tray pipeline pills navigate to Eval/Builds/Systems;
EvalDrawer policy fail cards link to the Policies view). It also reworks the
Edit System modal with tabbed layout (General/Deployment/Security/Danger zone)
and an in-modal SSH key rotation flow, adds cache-push status to the Builds
detail pane, and expands EvalDrawer policy fail cards with expandable config
attribute details.

The existing visual-drift audit task (`TASK-386`) targeted the earlier
2026-07-07 snapshot and does not cover this new delta. The builders task
(`TASK-393`) targets only the builders surface from `2cc51a2d`. A new scoped
task is needed to implement these cross-surface and modal changes on top of
both.

## Goal

Bring the shipped web UI into parity with the 2026-07-16 design delta from
commit `e0c7b724` for:

1. The cross-surface focus-prop navigation system: Dashboard git-graph badges
   navigating to Builds/Evals, Flakes-view pipeline pills navigating to
   Eval/Builds/Systems, EvalDrawer policy fail cards linking to Policy
   definitions and Systems, and SystemDetail FlakeTray completing its
   navigation callbacks.

2. The EditSystemModal tabbed redesign: General, Deployment, Security, and
   Danger zone tabs, including the in-browser SSH keypair generation
   UX (generate/paste, fingerprint preview, rotate-and-revoke flow).

3. Builds detail pane cache-push status section showing per-cache-destination
   push states.

4. EvalDrawer policy tab expansion: fail cards expand to show config attribute
   paths, assertion messages, and deep-links to policy definitions.

5. Fixture/data realism alignment: evals use real fleet hostnames, flake
   assignments and build statuses reflect more realistic distributions.

## Authoritative Commit Delta

- Commit: `e0c7b724` (`Design update`)
- Implement from the exact design-file changes in that commit:
  - `docs/design/CrystalForge/app.jsx`
  - `docs/design/CrystalForge/components/BuildsView.jsx`
  - `docs/design/CrystalForge/components/DashboardView.jsx`
  - `docs/design/CrystalForge/components/EditSystemModal.jsx`
  - `docs/design/CrystalForge/components/EvalDrawer.jsx`
  - `docs/design/CrystalForge/components/EvalsView.jsx`
  - `docs/design/CrystalForge/components/FlakesView.jsx`
  - `docs/design/CrystalForge/components/PoliciesView.jsx`
  - `docs/design/CrystalForge/components/SystemDetail.jsx`
  - `docs/design/CrystalForge/data-builds.js`
- Treat the commit diff as the authoritative source, but note that fixture data
  reshuffling (`.fixtures.js` / `.fixtures.json`) is design-example-internal
  and does not require direct porting; the parity target is the interaction and
  presentation behavior, not the exact fixture seed values.

## Non-Goals

- Re-doing the Builders side-panel work already covered by TASK-393.
- Backend/schema/API changes unless a tiny UI contract gap is discovered.
- Broad builder runtime/scheduling/auth fixes.
- Mobile/responsive redesign of any affected surface.
- Changes to surfaces not listed in the authoritative commit delta above,
  except shared web-ui primitives strictly required for this delta.
- Porting exact fixture seed values from the design example (fixture
  reshuffling is design-internal; the Rust test fixture layer has its own
  seed strategy).

## Scope Notes

This task is driven by the `e0c7b724` delta, which adds:

1. **App.jsx shell changes**: `EditSystemModal` now receives `initialFlake` and
   `onClearInitialFlake` props for the modal-tabbed flake assignment flow.

2. **DashboardView git-graph navigation**: Building/evaluating/failed badges on
   the git-graph commit rows are now clickable and navigate to Builds/Evals
   views with focus context (sha, flake, status). Failed badge differentiates
   eval-failure vs build-failure for correct navigation target.

3. **BuildsView focus-prop + cache-push details**: Accepts a `focus` prop for
   deep-linking from Dashboard git-graph. Build details pane gains a "Cache
   push status" section showing per-cache-destination push state for
   completed/cache-pushed/cache-pushing/failed builds, computed from the build's
   flake environments and the cache registry.

4. **EvalsView focus-prop**: Accepts `focus` and `onOpenSystem`/`onOpenPolicy`
   callbacks for cross-view navigation from Dashboard and Flakes. Focus prop
   selects the matching eval and opens its drawer.

5. **EvalDrawer policy tab expansion**: Fail/warn cards are now expandable.
   When expanded, shows the config attribute path
   (`nixosConfigurations.{host}.{attr}`), the plain-text assertion message,
   and a "View policy definition" button that deep-links to Policies view.
   Uses a `EVAL_CHECK_INFO` lookup table mapping short policy codes to human
   labels, descriptions, and policy IDs. Policy matrix rows now use real fleet
   hostnames from `SYSTEMS` data.

6. **EditSystemModal tabbed layout**: Split into General, Deployment, Security,
   Danger zone tabs. Security tab includes SSH key rotation flow with
   generate-keypair (client-side deterministic mock), paste-existing-public-key
   modes, fingerprint preview, and rotate-and-revoke confirmation. Tags field
   moved to General tab. Heartbeat interval moved to Deployment tab.

7. **FlakesView clickable pipeline pills**: PipelinePill and RolloutPill
   components accept `onClick` handlers. Tray-mounted pills navigate to Eval
   detail, Builds filtered to commit, or Systems filtered to flake.

8. **FlakesView focus-prop wiring**: Shell passes `onOpenEval`, `onOpenBuild`,
   `onOpenSystems` callbacks down through FlakesView → FlakeTray →
   pipeline pills.

9. **PoliciesView focus-prop**: Accepts a `focus` prop for deep-linking from
   EvalDrawer policy cards.

10. **SystemDetail FlakeTray callbacks**: Wires `onOpenEval`, `onOpenBuild`,
    `onOpenSystems` into the FlakeTray for the commit-peek drawer, enabling
    pipeline pill clicks from system detail.

11. **Fixture/data updates**: Eval mock generators use `SYSTEMS.filter(s =>
    s.flake === ev.flake).map(s => s.hostname)` for real hostnames instead of
    hardcoded lists. Build generator uses `Math.floor(r()*4)` for randomized
    flake assignment. History evals use randomized status distribution instead
    of round-robin.

## Architectural Constraints

- Follow the existing `packages/web-ui` view/component split per surface.
- No business logic in presentation components beyond view-local formatting.
- The focus-prop system should be lightweight (an optional prop + `useEffect`);
  do not introduce a global event bus or URL-based routing for focus context
  unless the existing routing layer already supports it.
- Reuse existing expandable-card and tab-bar patterns where they already exist
  in the codebase; do not introduce one-off interaction models.
- Keep scope limited to files listed in the authoritative delta and directly
  required shared styling/test/check files.
- SSH key rotation in Security tab MUST remain a mock/client-side-only flow in
  the parity implementation — real key rotation happens via backend API and is
  already handled separately.
- Any new check baselines or assets must be tracked in Git.

## Verification Plan

Automated:

- nix develop -c cargo fmt --manifest-path packages/web-ui/Cargo.toml --all -- --check
- nix develop -c cargo clippy --manifest-path packages/web-ui/Cargo.toml --all-targets -- -D warnings
- nix develop -c cargo test --manifest-path packages/web-ui/Cargo.toml
- nix build .#checks.x86_64-linux.web-ui --no-link

Manual:

- Compare Dashboard git-graph badge click navigation against
  `docs/design/CrystalForge/components/DashboardView.jsx` — building badge
  navigates to Builds, evaluating badge to Evals, failed badge to Builds or
  Evals depending on failure kind.
- Compare EditSystemModal tab layout and Security tab key-rotation UX against
  `docs/design/CrystalForge/components/EditSystemModal.jsx`.
- Compare Builds detail pane cache-push status section against
  `docs/design/CrystalForge/components/BuildsView.jsx`.
- Compare EvalDrawer policy tab expandable fail cards against
  `docs/design/CrystalForge/components/EvalDrawer.jsx`.
- Verify Flakes-tray pipeline pill clicks navigate to correct target views.
- Capture MR screenshots for each changed surface using deterministic
  local/web-ui-check output.

## Impact Areas

- `packages/web-ui/src/views/builds.rs`
- `packages/web-ui/src/views/evaluations.rs`
- `packages/web-ui/src/views/flakes_list.rs`
- `packages/web-ui/src/views/policies.rs`
- `packages/web-ui/src/views/dashboard.rs`
- `packages/web-ui/src/views/system_detail.rs`
- `packages/web-ui/src/components/system/edit_system_modal.rs`
- `packages/web-ui/src/components/evaluations/eval_drawer.rs`
- `packages/web-ui/src/components/flakes/flake_tray.rs`
- `packages/web-ui/assets/app.css` (shared styling for tab bars, expandable cards)
- `checks/web-ui/`

## Risk Level

Medium.

The change touches 7+ views and their components, making it the broadest
single delta in this design-parity wave. The focus-prop system requires
coordinated changes across the shell routing layer and individual views.
The EditSystemModal rewrite is the largest single-component change and carries
the highest risk of regressing the existing system edit flow. Mitigation:
implement view by view, run Tiers 0/1 after each view, and keep the build
green.

## Dependencies

- `TASK-393` must land first, per maintainer direction. This task implements
  the builders side-panel delta from `2cc51a2d`; the present task stacks
  cross-surface and modal changes on top of both the builders and non-builders
  2026-07-16 delta.

## Follow-Up Guidance

If implementation uncovers additional design drift outside the `e0c7b724`
delta, file a separate Backlog task instead of expanding this one.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria

<!-- AC:BEGIN -->
- [ ] #1 The shipped Dashboard git-graph badges (building, evaluating, failed) are clickable and navigate to the correct target view (Builds/Evals) with appropriate focus context (commit sha, flake, status)
- [ ] #2 Builds view accepts a focus prop from cross-view navigation and opens the matching build detail pane with the commit filtered; the cache-push status section appears in the build detail pane for completed/cache-pushing/cache-pushed/failed builds
- [ ] #3 Evals view accepts a focus prop and opens the matching eval drawer; EvalDrawer policy-tab fail cards are expandable, showing config attribute path, assertion message, and a "View policy definition" deep-link
- [ ] #4 EditSystemModal has the tabbed layout (General, Deployment, Security, Danger zone) matching the design reference, including the Security tab with generate/paste keypair UX and fingerprint preview
- [ ] #5 Flakes view pipeline pills (PipelinePill, RolloutPill) are clickable and navigate to Eval/Builds/Systems views with appropriate context; FlakeTray in SystemDetail also wires these callbacks
- [ ] #6 Policies view accepts a focus prop for deep-linking from EvalDrawer policy fail cards
- [ ] #7 Only files listed in the authoritative delta and directly required shared styling/check files are modified; unrelated behavior stays out of scope
- [ ] #8 `cargo fmt`, `cargo clippy -- -D warnings`, `cargo test`, and `nix build .#checks.x86_64-linux.web-ui --no-link` pass from the repository dev environment
- [ ] #9 Any newly discovered design drift outside this delta is filed as a separate Backlog task rather than implemented here
<!-- AC:END -->
