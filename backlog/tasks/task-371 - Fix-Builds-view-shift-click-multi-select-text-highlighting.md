---
id: TASK-371
title: Fix Builds view shift-click multi-select text highlighting
status: In Progress
assignee:
  - gpt-5.5
created_date: '2026-06-27 03:41'
updated_date: '2026-06-27 04:47'
labels:
  - builds
  - ui
  - selection
  - ux
  - bug
milestone: 'm-19: Design Parity Existing Surfaces'
dependencies: []
references:
  - packages/web-ui/src/views/builds.rs
  - packages/web-ui/src/components
  - checks/web-ui/tests/integration-test.js
  - TASK-371.1
modified_files:
  - packages/web-ui/src/components/builds/build_queue_pane.rs
  - packages/web-ui/assets/app.css
  - checks/web-ui/tests/integration-test.js
priority: medium
ordinal: 0
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

The Builds view supports multi-select interactions, but shift-clicking rows/cards currently allows the browser to select/highlight page text. This makes bulk selection feel broken and noisy because the user sees large chunks of text highlighted while trying to select build items.

## Goal

Make shift-click multi-select on the Builds view behave like an application selection interaction: users can select ranges without accidental browser text highlighting.

## Non-Goals

- Redesigning the Builds view layout
- Reworking unrelated selection behavior in other views
- Changing build queue backend behavior
- Adding new bulk actions beyond fixing the existing selection interaction

## Architectural Constraints

- Keep the fix in the Builds view UI/component layer only.
- Do not add business logic to presentation components.
- Prefer a targeted CSS/event-handling fix over broad global `user-select: none` changes.
- Preserve normal text selection in areas where users reasonably need to copy text, unless that area is part of the row/card selection target.
- Follow existing Dioxus component patterns.

## Impact Areas

- Builds view row/card selection interaction
- Builds view styling for selectable elements
- Any shared list/table component used by the Builds view, if that is where selection behavior is implemented

## Risk Level

Low-medium: likely a localized UI interaction fix, but careless CSS could prevent useful text selection elsewhere.

## Verification Plan

- Add or update a targeted UI/browser interaction test if the project has coverage for Builds view selection.
- Manually verify in the web UI:
  - single click selection still works
  - shift-click range/multi-select works
  - shift-click no longer highlights build row/card text
  - ordinary text selection outside the selectable build item remains possible where applicable
- For UI change MR, include screenshot or recording from the `web-ui` check if the task proceeds to implementation.
- Run changed frontend formatting/check commands, for example:
  - `nix develop -c cargo fmt --manifest-path packages/web-ui/Cargo.toml -- --check` or changed-file rustfmt equivalent
  - `nix develop -c cargo check --manifest-path packages/web-ui/Cargo.toml --target wasm32-unknown-unknown`
  - relevant `web-ui` check if needed by MR verification

## Proposed Approach

1. Locate the Builds view selection target(s) and the multi-select event handlers.
2. Prevent browser text selection only during row/card selection interactions, likely via scoped CSS (`user-select: none`) on selectable build rows/cards and/or `prevent_default` on shift-click selection events.
3. Ensure nested controls/links/buttons still behave normally and do not accidentally trigger range selection.
4. Add/adjust a targeted interaction test if practical, otherwise document manual verification steps clearly in the MR.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Shift-click multi-select in the Builds view no longer highlights/selects row or card text.
- [x] #2 Existing single-select and shift-click range/multi-select behavior continues to work.
- [x] #3 The fix is scoped to Builds view selectable items or their shared selection component and does not globally disable text selection across the app.
- [x] #4 Nested buttons, links, and controls in build rows/cards remain usable and do not accidentally trigger unwanted range selection.
- [x] #5 Manual verification or a targeted UI interaction test covers the shift-click behavior.
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented scoped Builds queue fix: added `prevent_default()` for shift-click mousedown/click on cancellable queue rows and scoped `user-select: none` to `.q-queue-table .q-row.selectable` only.

Added web-ui check step `15i-builds-shift-click-selection` to ci_fast screenshots. It shift-clicks two build queue rows, asserts `2 selected`, asserts `window.getSelection()` remains empty, and produces `15i-builds-shift-click-selection.png`.

Verification results:
- PASS `nix develop -c rustfmt --edition 2024 --check packages/web-ui/src/components/builds/build_queue_pane.rs`
- PASS `nix develop -c cargo check --manifest-path packages/web-ui/Cargo.toml --target wasm32-unknown-unknown`
- PASS `nix develop -c cargo test --manifest-path packages/web-ui/Cargo.toml`
- PASS `nix develop -c node --check checks/web-ui/tests/integration-test.js`
- PASS `nix build .#checks.x86_64-linux.web-ui --print-out-paths` -> `/nix/store/vj8bclimmfmz06ca8g69c3cqi0y6q57j-vm-test-run-crystal-forge-web-ui-mega-integration`
- Screenshot artifact present: `result/screenshots/15i-builds-shift-click-selection.png`

Package-wide `cargo fmt --manifest-path packages/web-ui/Cargo.toml -- --check` remains blocked by pre-existing unrelated formatting drift in `packages/web-ui/src/export/mod.rs` and `packages/web-ui/src/views/compliance.rs`; follow-up TASK-371.1 created in Backlog.
<!-- SECTION:NOTES:END -->
