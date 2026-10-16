---
id: TASK-488
title: Match the Evaluation drawer pixel-for-pixel to the design example
status: In Progress
assignee:
  - opencode-claude
created_date: '2026-10-16 00:27'
updated_date: '2026-10-16 01:37'
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

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Reuse and verify the task-owned devenv preview; verify fixture login and an actual Evaluation detail drawer before edits.
2. Capture the unchanged React reference and production drawer with browser-local matched evaluation identities. Record bounding boxes, computed styles, and paired screenshots without masking differences.
3. Correct presentation in small batches: shell/header/log metrics first, then populated logs, policy details and graph presentation. Keep TASK-466 bundle matrix work untouched.
4. Extend comparison coverage to lifecycle/loading/error states and authoritative browser integration. Do not label typography-only assertions as pixel parity.
5. Run changed-file formatting, UI unit tests, WASM check and focused browser checks. Retain precise gaps and leave the live preview running. No commit, push or MR without user authorization.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
LOCK: opencode-claude on sledge in /home/mcamp/code/crystal-forge/TASK-488-eval-drawer-pixel-parity

Preflight: branch TASK-488-eval-drawer-pixel-parity was created from the fetched origin/dev at 33de73b013b735304dfac9c69745a074a3e76ebd (merge of TASK-470). Local dev was not used as the base because it is 544 backlog-only commits ahead and 10 behind origin/dev. The new worktree is clean, 0 ahead and 0 behind origin/dev, and its upstream was unset so a bare push cannot target dev. The dev integration worktree is clean except for the owner-preserved untracked TASK-478.lock and session-ses_f927.md. No main worktree is registered. Task selected by the user on 2026-10-16 ("do what needs to be done to get to work on doing a focused pixel for pixel exact matching of the design eval drawer"); this was treated as moving the task to To Do and starting it.

Coordination: TASK-466 (policy-matrix data and CSS) has 17 uncommitted files in /home/mcamp/code/crystal-forge/TASK-466-eval-policy-matrix-bundles, including packages/web-ui/src/views/evaluations.rs and assets/app.css, and a migration numbered 0269 that now collides with origin/dev's 0269_scanning_lifecycle_archive.sql. This task does not touch that worktree. Expect rebase conflicts in evaluations.rs and app.css when TASK-466 lands.

Preview started on 2026-10-16 from /home/mcamp/code/crystal-forge/TASK-488-eval-drawer-pixel-parity with `nix develop .#devenv --command devenv up -d` (log /tmp/opencode/task488/devenv-up.log). `devenv processes list` reports api, postgres, and web as ready with 0 restarts. Process cwd values are inside this worktree: PostgreSQL 17 pid 31192 (data directory .devenv/state/postgres, 127.0.0.1:5432), cf-server-core pid 31379 (0.0.0.0:3445), dx 0.7.3 pid 31569 (127.0.0.1:8080). Browser URL http://127.0.0.1:8080/. GET / returned 200 on :8080 and GET /status returned 200 on :3445. The API seeded the design golden fixture (35 systems, 48 CVEs, 5 builders, 175 system events) with mock execution mode. Local fixture login is the devenv bootstrap admin from devenv.nix (a development fixture credential, not a real one).

Known gaps at preview start: (1) the API listener binds 0.0.0.0:3445 although the generated config says server.host 127.0.0.1; this was observed and not changed, and no firewall change was made. (2) The seed log says build_jobs and hardening seeding were skipped (needs derivations FK), so the evaluation queue, history, log, policy, and graph data for the drawer may not match the design fixture. This must be checked before any pixel comparison. (3) Login, the /evaluations route, and an open drawer have not yet been verified in a browser. (4) ImageMagick (identify, compare) is not on the default PATH; the parity harness needs it inside nix develop.

First implementation batch: added drawer-scoped header typography/spacing, 11px monospace identity metadata, design evaluation glyph, design 12.5px timestamp typography, native tab font metrics and 11px/1.55 log metrics. Existing cancellation, retry, polling and policy state logic is unchanged. Only evaluations.rs, icon.rs, app.css plus the new capture-eval-drawer-shell.cjs and eval-drawer.md are changed. No commit or push.

Browser baseline verified local fixture login HTTP 200 and real Completed evaluation drawer. Reference is the repository app's Nix-built offline design at /nix/store/97akjbrnqsvrmg868cf6m1z61x6bvk5i-cf-design-example-offline. New comparison mounts the unchanged reference component with matched queue identity and intercepts only production queue/log responses in an isolated browser context. It closes Coach using its real accessible button when present; asynchronous saved preference can remove Coach, so the normal interaction is retried for that race. No force clicks or hidden screenshot differences.

Focused browser command: NODE_PATH=/nix/store/fg778x0631ld4xd8fim2kbvxll5715xp-playwright-test-1.59.1/lib/node_modules PLAYWRIGHT_BROWSERS_PATH=/nix/store/f0rap655j6wmqbfvqdw445kwcxkxwf7n-playwright-browsers /nix/store/l07gdbxylzfl8pbx9pxy6fyg95w4hjwy-nodejs-24.19.0/bin/node checks/web-ui/design-parity/capture-eval-drawer-shell.cjs /nix/store/97akjbrnqsvrmg868cf6m1z61x6bvk5i-cf-design-example-offline /tmp/opencode/task488/shell-first-pass. Actual exit 0: desktop dark, desktop light, 720px dark all pass selected typography, metadata color and header/tab height assertions. Paired screenshots and JSON measurements are saved there. This does NOT verify full pixel parity or authoritative VM coverage.

Verification: changed-file `rustfmt --edition 2024 --check .../icon.rs .../evaluations.rs` passed through nix develop. Initial cargo test --lib command was invalid because web-ui is a binary target; corrected `CARGO_BUILD_JOBS=2 cargo test --manifest-path packages/web-ui/Cargo.toml` passed 677 tests, 0 failed, 1 ignored. Subsequent `CARGO_BUILD_JOBS=2 cargo check --manifest-path packages/web-ui/Cargo.toml --target wasm32-unknown-unknown` passed. Combined command actual exit 0; retained output /home/mcamp/.local/share/opencode/tool-output/tool_14237dab6001kYEX6qvhQErToO. Package-wide cargo fmt check fails on pre-existing caches.rs/cves.rs/poams.rs formatting; those files are untouched (existing TASK-371.1 tracks formatting drift). git diff --check passed.

Preview remains available at http://127.0.0.1:8080/evaluations. Task-owned frontend was restarted using devenv processes restart web and the new glyph/styles were verified in browser. Backend and database were not restarted. Remaining visual differences include timestamps/DTG rendering, status wording, action availability and missing expand/confirmation controls, development chrome offsets, log toolbar metrics and empty-state differences. Policy/graph comparison, populated log fixtures, pixel-diff reporting and authoritative browser integration are still pending. TASK-488 remains In Progress.

User requested 0.0.0.0:8080. Applied runtime-only devenv process overrides: original web exec with --addr changed to 0.0.0.0; API exec allows exact CORS origin http://10.8.0.177:8080. No repository config or firewall changes. Restarting a single process did not reload its exec override. Restarted only this task's stack; one restart timed out, then verified task-owned manager cwd and stopped it gracefully before evaluating ports with listeners free. Final startup succeeded; frontend PID10773 listens 0.0.0.0:8080 and LAN-address GET returns 200. Existing task database preserved. Runtime-only overrides must be retained when restarting the stack; normal default startup reverts to loopback.

Network preview URL: http://10.8.0.177:8080/evaluations (host sledge). Exact-origin CORS preflight succeeds. Browser login over LAN HTTP remains blocked: login returns HTTP200 with a Secure Set-Cookie, but Chromium stores no cookie on non-loopback plain HTTP. No cookie security was weakened. Need HTTPS for direct network login, or SSH-forward UI8080 and API3445 and browse localhost. Loopback browser preview remains the supported authenticated review path.
<!-- SECTION:NOTES:END -->
