---
id: TASK-379
title: >-
  Web UI design-parity harness: compare Dioxus screenshots against
  design-example targets from shared fixtures
status: Backlog
assignee: []
created_date: '2026-07-03 20:03'
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
  - checks/web-ui
  - docs/design/CrystalForge
  - docs/design/CrystalForge/fixtures/README.md
  - checks/web-ui/coverage-manifest.json
  - checks/web-ui/design-fixtures.json
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/292'
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
