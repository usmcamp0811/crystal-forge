---
id: TASK-379
title: >-
  Web UI design-parity harness: compare Dioxus screenshots against
  design-example targets from shared fixtures
status: Review
assignee: []
created_date: '2026-07-03 20:03'
updated_date: '2026-07-04 01:37'
labels:
  - web-ui
  - ci
  - nix
  - playwright
  - visual-regression
  - design-parity
  - developer-experience
milestone: ui-views-system-detail
dependencies: []
references:
  - checks/web-ui/design-parity
  - docs/design/CrystalForge
  - docs/design/CrystalForge/fixtures/README.md
  - checks/web-ui/coverage-manifest.json
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/292'
modified_files:
  - docs/design/CrystalForge/app.jsx
  - checks/web-ui/design-parity/manifest.json
  - checks/web-ui/design-parity/generate-design-targets.js
  - checks/web-ui/design-parity/compare-design-parity.js
  - checks/web-ui/tests/integration-test.js
  - checks/web-ui/default.nix
  - .gitlab-ci.yml
  - docs/web-ui-check.md
priority: high
ordinal: 323000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

The web-ui check now captures real Dioxus screenshots, but there is no objective, data-aligned way to measure how closely the shipped UI matches the tracked design gold standard under `docs/design/CrystalForge`. The design example was updated to render from a shared canonical fixture (`docs/design/CrystalForge/fixtures/crystal-forge.fixtures.json`, exposed to the example via `crystal-forge.fixtures.js` / `window.__CF_FIXTURES`).

## Desired Outcome

A script/package that:
1. Renders the design example (React) for a specific view + theme, backed by the shared fixture, and saves a "design target" screenshot.
2. Has the web-ui check render the same real Dioxus view backed by the same fixture data, and saves the Dioxus screenshot.
3. Compares the two screenshots and captures a design-drift metric per view/theme.

For now this MUST NOT fail the web-ui check; it is a non-blocking design-parity gauge whose metric and thumbnails are surfaced in the MR so drift is visible.

## Agreed Approach (from grooming)

- Rendering: add a small deterministic view-selector hook to the design example (e.g. `?view=&theme=` / `window.__CF_TARGET`) so Playwright can render a specific view+theme headlessly and screenshot it.
- Metric: perceptual + structural similarity score (e.g. ImageMagick diff ratio + normalized/resized compare) per view/theme; report a number + thumbnails in the MR; never fail.
- Location/scope: new `checks/web-ui/design-parity/` harness wired into the existing web-ui check. Start with primary full-page views (dashboard, systems, builds, cves, flakes, environments, caches, policies, compliance, scanning, builders, admin) in both dark and light themes, then expand.

## Non-Goals

- Do not require pixel-identical parity; React vs Dioxus will differ.
- Do not make design drift merge-blocking in this iteration.
- Do not rewrite the design example beyond the minimal deterministic view/theme selection hook and fixture wiring already present.
- Do not attempt every modal/interaction sub-state; focus on primary views first.

## Notes / Context

- Design example entry: `docs/design/CrystalForge/crystal-forge.html` (client React via Babel standalone, no URL routing; `topView` + `theme` are React state; content div carries `data-screen-label`).
- Fixture contract + field docs: `docs/design/CrystalForge/fixtures/README.md`.
- Manifest already maps 109/117 steps to `designRef` components in `checks/web-ui/coverage-manifest.json`.
- The real Dioxus web-ui check already pins viewport 1920x1080, UTC, en-US, and captures dark+light themed screenshots.
- The Dioxus app must be backed by the same fixture data for the compared views; this likely requires routing the relevant API endpoints to fixture-derived responses in the harnessed steps (mirroring existing `page.route` mocks) so both sides render the same records.
<!-- SECTION:DESCRIPTION:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented on the TASK-139 branch / MR !292 (commit ac7e3028).

Design-parity harness (non-blocking):
- Added a backward-compatible `?view=&theme=` (and `window.__CF_TARGET`) render hook to `docs/design/CrystalForge/app.jsx`; seeds initial topView/theme/density/sidebar and suppresses the setup coach overlay for clean parity screenshots. No behavior change when no params are provided.
- Added `checks/web-ui/design-parity/manifest.json` mapping 13 primary views to (design example view name) + (real Dioxus route), themes dark+light, non-blocking, ImageMagick RMSE normalized compare (resizeWidth 960).
- `generate-design-targets.js`: Playwright renders the offline design example per view/theme -> `<view>--<theme>.design.png`.
- `compare-design-parity.js`: normalizes both sides (resize+flatten) and scores with `compare -metric RMSE`; writes `design-drift-report.json`, `design-drift-summary.md`, and side-by-side `montages/`. Wrapped so it never fails the check.
- `integration-test.js`: new Phase capturing real Dioxus parity screenshots (`<view>--<theme>.dioxus.png`) by seeding `cf.ui.theme` then loading each route so the app applies its own theme via the real CF theme path.
- `default.nix`: vendors React/ReactDOM/Babel via pinned `fetchurl` (SRI from crystal-forge.html) and builds an offline design bundle (`designExampleOffline`) with CDN <script> tags rewritten to local files, so the example renders with no network in the VM. Added Phase 4c to run generator + comparison and copy artifacts (report, summary, montages, design-targets, design-parity) into $out/screenshots. Non-blocking.
- `.gitlab-ci.yml`: MR comment now prepends the design-drift summary and uploads up to 26 montages.
- Documented the harness in `docs/web-ui-check.md`.

Lightweight validation only (no heavy builds per instruction): nix-instantiate --parse, node --check on all JS, manifest JSON parse, git diff --check — all pass.

Pending: authoritative MR !292 `flake-check: [web-ui]` run to confirm the offline design example renders and the drift metric/montages are produced. First-iteration scope note: Dioxus captures currently use the app's own route rendering (real backend/seeded data); exact byte-for-byte fixture alignment of the Dioxus side (routing APIs to fixture-derived responses) is a candidate follow-up refinement.

CI pipeline 2650887037 (commit 6f1bcf82) passed — flake-check: [web-ui] succeeded in 757s.

Final harness results:
- 26/26 views compared (13 views × dark + light), 0 missing, 0 errors
- Average similarity: 88.0% (avgDrift 0.1204)
- Worst: environments--light (drift 0.22); light mode consistently drifts more than dark (~0.13 vs ~0.10)
- design-parity-matrix.png grid image generated and uploaded to MR comment
- All non-blocking — check never fails on visual mismatch

Also shipped in this batch:
- Design example IS now the visual baseline (deleted 106 stored Dioxus-vs-Dioxus PNGs)
- nix run .#generate-design-targets -- --out-dir <dir> [--fixtures <json>] app added
- bash docs/design/CrystalForge/serve.sh for local preview
- --allow-file-access-from-files Chromium flag fixed the black left-side montage issue
- sha256 hashes replaced invalid sha384 hashes in fetchurl (Nix doesn't support sha384)
<!-- SECTION:NOTES:END -->
