---
id: TASK-474
title: Restore cargo fmt --check cleanliness in views/poams.rs and views/cves.rs
status: Backlog
assignee: []
created_date: '2026-10-03 00:23'
labels:
  - web-ui
  - formatting
dependencies: []
references:
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/329'
modified_files:
  - packages/web-ui/src/views/poams.rs
  - packages/web-ui/src/views/cves.rs
priority: low
type: chore
ordinal: 495000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
cargo fmt --manifest-path packages/web-ui/Cargo.toml -- --check fails with 4 hunks in src/views/poams.rs and 2 hunks in src/views/cves.rs. views/cves.rs passes at the base branch dev, and views/poams.rs does not exist there, so the drift comes from earlier commits on MR !329. No CI job or flake check currently enforces formatting, so nothing fails today, but earlier checkpoint reports named a bare `rustfmt` run, which defaults to edition 2015 and fails on async code, and never checked formatting. Format-only; behavior must not change.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 cargo fmt --manifest-path packages/web-ui/Cargo.toml -- --check exits 0.
- [ ] #2 The change contains formatting differences only, with no behavior change, confirmed by an unchanged cargo check for wasm32-unknown-unknown and unchanged web-ui tests.
- [ ] #3 The format command through the crate manifest, not a bare rustfmt call, is recorded as the verification command.
<!-- AC:END -->
