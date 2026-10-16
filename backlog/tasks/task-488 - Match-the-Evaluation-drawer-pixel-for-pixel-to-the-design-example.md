---
id: TASK-488
title: Match the Evaluation drawer pixel-for-pixel to the design example
status: To Do
assignee: []
created_date: '2026-10-16 00:27'
labels:
  - web-ui
  - design-parity
  - evaluations
dependencies: []
references:
  - packages/web-ui/src/views/evaluations.rs
  - checks/web-ui/design-parity/manifest.json
  - checks/web-ui/design-parity/compare-design-parity.js
  - TASK-466
  - TASK-441
  - TASK-447
  - TASK-386
documentation:
  - docs/design/CrystalForge/components/EvalDrawer.jsx
  - docs/design/CrystalForge/styles.css
  - docs/design/CrystalForge/serve.sh
  - docs/design/CrystalForge/fixtures/README.md
priority: high
type: enhancement
ordinal: 510000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem
The production Evaluation drawer (`packages/web-ui/src/views/evaluations.rs`: `EvalDrawer`, `EvalDrawerLogTab*`, `EvalDrawerPolicyTab`, `EvalDrawerGraphTab`) has not been compared element by element against the design drawer. Earlier parity work (TASK-345.1, TASK-394, TASK-284) was judged by eye against older snapshots. The existing parity harness (`checks/web-ui/design-parity`) captures only the page-level `/evaluations` route and uses an advisory RMSE score. It never opens the drawer.

## Goal
With the same viewport, theme, and fixture data, each drawer state in production renders identically to the design example: same geometry, spacing, typography, colors, icons, borders, and state styling. "Identical" is measured by a repeatable element-level and pixel-level comparison, not by eye. Any residual difference must be one that real production data causes or one the design cannot express, and each such difference is written down with its reason.

## Scope (design source `docs/design/CrystalForge/components/EvalDrawer.jsx`, 804 lines, plus matching rules in `docs/design/CrystalForge/styles.css`)
- Drawer shell and header (`EvalDrawer`, lines 3-107): backdrop, width, header, status chip, tab bar, close and cancel actions, live state.
- Log tab (`EvalLogTab`, lines 108-146), including live and finished states.
- Policy tab (`EvalPolicyTab`, lines 163-588) except the bundle-grouped matrix: top issues, host health, fail and warn cards, expanded config attribute detail, "View policy definition" link.
- Graph tab (`EvalGraphTab`, lines 589-657).
- Force-cancel confirmation (`ConfirmForceCancel`, lines 658-684).
- Empty, loading, and error states of each tab, desktop light, desktop dark, and a narrow dark viewport.

## Non-Goals
- Policy-matrix bundle grouping, bundle provenance persistence, or any API or migration change (owned by TASK-466, in progress, uncommitted in its own worktree). The visual parity of the bundle matrix is rechecked after TASK-466 merges.
- Dependency-graph build counts and scaling (owned by TASK-441).
- Evaluator, scheduling, deployment-gating, scanning, or CVE behavior.
- Redesigning the design example. The design is the reference and is not edited.
- Changing behavior, data flow, or routes to make a screenshot match. Presentational changes only, unless a difference cannot be fixed without one (then file a follow-up task).

## Architectural Constraints
- Only `packages/web-ui` view, component, and CSS files change, plus `checks/web-ui` manifests, tests, and baselines.
- Shared styles go in `packages/web-ui/assets/app.css`. Port the design rules instead of inventing a parallel visual system.
- Keep rendering code focused on presentation and event wiring. Do not move domain decisions into view markup.
- Browser/WASM-compatible code only.
- An intentional visual change that breaks a browser assertion or baseline updates it in the same MR.

## Method
1. Serve the design with `docs/design/CrystalForge/serve.sh` using the shared golden fixture. Run production with the task preview and mock execution. Seed production so the same evaluation, systems, policies, and log lines appear as in the fixture.
2. For each drawer state, capture both apps at the same viewport and device scale factor.
3. Compare computed styles and bounding boxes per matched element, then compare pixels of the drawer region. Record the result per state.
4. Fix each difference at its source. Re-run until the report shows no unexplained difference.

## Verification Plan
- Extend `checks/web-ui/design-parity/manifest.json` (or add a drawer-specific manifest) so that open-drawer states are captured for both design and production.
- Add a focused browser workflow in `checks/web-ui` that opens the drawer in each tab and asserts the matched element geometry and key computed styles.
- Run focused checks first: `cargo fmt --check`, `cargo test` and `cargo check --target wasm32-unknown-unknown` in `packages/web-ui` through `nix develop`, then the focused browser workflow. The full `web-ui` suite and `nix flake check` are not needed to preview an edit.
- Final: the authoritative `web-ui` check (or exact-head CI where the task permits it), plus MR screenshots (design beside production) for every state.

## Impact Areas
`packages/web-ui/src/views/evaluations.rs`, `packages/web-ui/src/components/eval_log_modal.rs`, `packages/web-ui/assets/app.css`, `checks/web-ui/design-parity/*`, `checks/web-ui/tests/*`, `checks/web-ui/coverage-manifest.json`.

## Risk
Medium. Presentational only, but the drawer file is about 3,200 lines and TASK-466 and TASK-441 edit the same file and `app.css`. Expect rebase conflicts. Baselines for existing browser steps may need updates. `ImageMagick` is required by the existing harness and is not on the default shell PATH, so run it inside `nix develop`.

## Dependencies
None blocking. Related and potentially conflicting: TASK-466 (policy-matrix data and CSS), TASK-441 (dependency graph tab), TASK-447 (bundle-scaled policy results), TASK-386 (all-views drift audit, surface 7).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A repeatable comparison captures the design drawer and the production drawer at the same viewport, device scale factor, theme, and fixture data for: header and shell, Log (live and finished), Policy (excluding the bundle matrix), Graph, force-cancel confirmation, and each empty, loading, and error state
- [ ] #2 The comparison records, per state, matched-element bounding boxes and key computed styles (font, size, weight, color, spacing, border, radius) and a pixel comparison of the drawer region
- [ ] #3 Desktop dark, desktop light, and a narrow dark viewport have no unexplained difference in any in-scope state
- [ ] #4 Every remaining difference is listed in the task notes with its cause (real production data or a design limitation) and an owner or follow-up task
- [ ] #5 Styles come from ported design rules in assets/app.css, with no parallel visual system and no inline duplicates
- [ ] #6 No API, migration, scheduling, or behavior change is introduced; non-presentational gaps are filed as follow-up tasks
- [ ] #7 A focused browser workflow asserts the matched geometry and key computed styles of each in-scope state and passes in the authoritative web-ui check
- [ ] #8 Formatting, web-ui unit tests, the WASM check, and git diff --check pass
- [ ] #9 MR shows design and production screenshots side by side for every in-scope state
<!-- AC:END -->

## Definition of Done
<!-- DOD:BEGIN -->
- [ ] #1 Preview URL, review path, backend freshness, and data mode reported at handoff
<!-- DOD:END -->
