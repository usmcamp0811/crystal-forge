---
id: TASK-394
title: >-
  Implement the 2026-07-18 Builds/Evals infinite-scroll design delta
status: Review
assignee: [agent]
created_date: '2026-07-18 00:00'
updated_date: '2026-07-18 00:00'
mr: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/303
lock: awaiting-review
labels:
  - design-parity
  - web-ui
  - builds
  - evals
dependencies: []
references:
  - commit 010a77c5 (`add build/eval infinite scroll design update`)
  - docs/design/CrystalForge/components/BuildsView.jsx
  - docs/design/CrystalForge/components/EvalsView.jsx
  - docs/design/CrystalForge/components/Shell.jsx
  - docs/design/CrystalForge/data-builds.js
  - docs/design/CrystalForge/styles.css
  - packages/web-ui/src/views/builds.rs
  - packages/web-ui/src/views/evals.rs
  - packages/web-ui/assets/app.css
documentation: []
priority: high
ordinal: 394000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem Statement

The design example was updated in commit `010a77c5` (`add build/eval infinite
scroll design update`) on 2026-07-18 to add infinite-scroll pagination to the
Builds and Evaluations views. Without this change, large build/eval history
lists render entirely on load, which harms performance and user experience on
views with hundreds of entries.

The current shipped builds and evals views render the full list of entries
inline. The design now calls for a paginated approach: render an initial page
and progressively load more items as the user scrolls, using a sentinel element
at the bottom that triggers the next page when it scrolls into view.

## Goal

Bring the Builds and Evaluations views into parity with the 2026-07-18 design
delta from commit `010a77c5`: add the `useInfiniteScroll` hook, apply it to
both the Builds active/history tables and the Evals active queue and history
table, add the infinite-scroll sentinel styling and element, and update the
mock data sizes to demonstrate paging behavior.

## Authoritative Commit Delta

- Commit: `010a77c5` (`add build/eval infinite scroll design update`)
- Implement from the exact design-file changes in that commit:
  - `docs/design/CrystalForge/components/Shell.jsx` (add `useInfiniteScroll` hook)
  - `docs/design/CrystalForge/components/BuildsView.jsx` (integrate paging into build queue table)
  - `docs/design/CrystalForge/components/EvalsView.jsx` (integrate paging into active queue and history)
  - `docs/design/CrystalForge/data-builds.js` (increase mock data volume to demonstrate paging)
  - `docs/design/CrystalForge/styles.css` (add `.infinite-sentinel` style)
- Treat the commit diff as the authoritative source for what changed in scope.

## Non-Goals

- Full redesign of the Builds or Evals views beyond the infinite-scroll changes.
- Backend/API/schema changes — the design delta is UI-only.
- Adding infinite scroll to other views (Systems, Flakes, Caches, etc.) unless
  they already have a paging mechanism that must be adjusted for consistency.
- Changing the existing multi-select, bulk-action, or drawer behavior.
- Any mock data beyond what is needed to demonstrate the paging behavior
  (not for production use).
- Shared web-ui primitives beyond the `useInfiniteScroll` hook and the
  `.infinite-sentinel` CSS class.

## Scope Notes

This task is driven specifically by the delta in commit `010a77c5`, which adds:

- A new `useInfiniteScroll(resetKey, pageSize?)` hook in `Shell.jsx` that
  renders an initial page and grows the count when a sentinel element scrolls
  into view. It resets to the first page whenever `resetKey` changes (tab
  switch, new search/filter).
- Integration in `BuildsView.jsx`: the build list (both active and history
  tabs) is sliced to the current page count, and a sentinel `<div>` is
  rendered below the table when more items are available.
- Integration in `EvalsView.jsx`: the active queue and history table each get
  their own `useInfiniteScroll` instance, with independent paging state and
  sentinels. The `EvalHistory` component gains `hasMore`, `sentinelRef`, and
  `totalCount` props.
- Update to `data-builds.js`: increase `HISTORY_BUILDS` from 40 to 220 items
  and `HISTORY_EVALS` from 50 to 180 items so the paging behavior is visible.
- New `.infinite-sentinel` CSS class in `styles.css` for the loading/scroll
  indicator.

## Architectural Constraints

- Follow the existing `packages/web-ui` view/component split for builds and
  evals.
- The `useInfiniteScroll` hook should be placed in a shared module rather than
  duplicated across views. Mirror the placement in the design reference
  (`Shell.jsx` → shared layout/utility area).
- The infinite scroll must integrate with the existing search/filter and tab
  switching so that paging resets appropriately.
- Keep scope limited to the Builds and Evals views and the directly required
  shared hook and style additions.
- Infinite scroll should be passive (no active DB queries in flight per the
  design) — it operates on the already-loaded data list.

## Verification Plan

Automated:

- `nix develop -c cargo fmt --manifest-path packages/web-ui/Cargo.toml --all -- --check`
- `nix develop -c cargo clippy --manifest-path packages/web-ui/Cargo.toml --all-targets -- -D warnings`
- `nix develop -c cargo test --manifest-path packages/web-ui/Cargo.toml`
- `nix build .#checks.x86_64-linux.web-ui --no-link`

Manual:

- Compare the implemented Builds and Evals views against the updated reference
  in `docs/design/CrystalForge/components/BuildsView.jsx` and
  `docs/design/CrystalForge/components/EvalsView.jsx` at desktop width.
- Verify that scrolling to the bottom of the build history table loads more
  entries and the sentinel text ("Loading more builds…") appears.
- Verify that scrolling to the bottom of the evals history table loads more
  entries and the sentinel text ("Loading more…") appears.
- Verify that the active queue also pages (though typically short enough to
  fit on one page).
- Verify that switching tabs, applying a search query, or changing history
  filters resets the paging to the first page.
- Capture MR screenshots for the changed Builds and Evals states using
  deterministic local/web-ui-check output.

## Impact Areas

- `packages/web-ui/src/views/builds.rs`
- `packages/web-ui/src/views/evals.rs`
- `packages/web-ui/src/components/` (new shared `useInfiniteScroll` hook)
- `packages/web-ui/assets/app.css`
- `checks/web-ui/`

## Risk Level

Low-Medium.

The change is UI-focused and scoped to two views plus a shared hook. The main
risk is regressing existing multi-select, bulk actions, or drawer behavior in
Builds/Evals when the list rendering changes to use a paged slice.

## Dependencies

None.

This is a standalone design-delta task against the current shipped Builds and
Evals views. It should stack after any in-progress work that touches the same
view files to avoid merge conflicts.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria

<!-- AC:BEGIN -->
- [ ] #1 The `useInfiniteScroll` hook exists as a shared utility in the web-ui package and matches the design reference behavior: renders `pageSize` items initially, grows by `pageSize` when the sentinel scrolls into view (with a 400px lead-in), and resets to the first page when `resetKey` changes
- [ ] #2 The Builds view uses `useInfiniteScroll` for both the Active and Completed tabs, slicing the filtered list and rendering a "Loading more builds…" sentinel below the table when more items are available
- [ ] #3 The Evals view uses `useInfiniteScroll` independently for the Active Queue and the History table, each with its own sentinel and "Loading more…" indicator
- [ ] #4 The `EvalHistory` component receives and renders the `hasMore`, `sentinelRef`, and `totalCount` props per the design reference
- [ ] #5 The `.infinite-sentinel` CSS class is added to the app stylesheet matching the design: centered text, muted color, 14px padding
- [ ] #6 Mock data in `data-builds.js` (or the Rust equivalent) is increased to demonstrate paging — enough entries to require scrolling past the initial page
- [ ] #7 Switching tabs, typing a search query, or changing history filters resets the infinite scroll to the first page
- [ ] #8 Existing multi-select, bulk-action, and drawer behavior in Builds and Evals still works correctly with the paged list
- [ ] #9 `cargo fmt`, `cargo clippy -- -D warnings`, `cargo test`, and `nix build .#checks.x86_64-linux.web-ui --no-link` pass from the repository dev environment
- [ ] #10 Only Builds, Evals, shared hook, and stylesheet files are modified; unrelated surfaces stay out of scope
<!-- AC:END -->
