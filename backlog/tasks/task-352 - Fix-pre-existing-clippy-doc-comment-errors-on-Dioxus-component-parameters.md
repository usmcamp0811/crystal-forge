---
id: TASK-352
title: Fix pre-existing clippy doc-comment errors on Dioxus component parameters
status: Backlog
assignee: []
created_date: '2026-06-13 14:20'
labels:
  - tech-debt
  - web-ui
  - clippy
dependencies: []
ordinal: 298000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem
`cargo clippy` on packages/web-ui fails with `error: documentation comments cannot be applied to function parameters` in several Dioxus component files, e.g. `src/components/forms/add_system_form.rs` (lines ~44-60) and `src/components/modals/key_pair_modal.rs` (~line 53). These cascade into spurious `E0425 cannot find function use_signal` errors elsewhere (e.g. system_detail.rs) when the crate fails to compile under clippy.

`cargo check` currently passes, so these are clippy-level failures only, but they block a clean `cargo clippy -- -D warnings` gate.

## Desired Outcome
`cargo clippy` runs clean on packages/web-ui. Doc comments on `#[component]` function parameters are converted to non-doc comments (`//`) or moved, without changing rendered behavior.

## Notes
- Discovered while working TASK-295 (system detail tab icons). These errors are pre-existing on dev and unrelated to that change, so they were left out of scope.
- Likely affects multiple component files; grep for `/// ` immediately preceding component fn params.
<!-- SECTION:DESCRIPTION:END -->
