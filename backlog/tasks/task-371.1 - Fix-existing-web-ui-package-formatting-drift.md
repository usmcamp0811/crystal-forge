---
id: TASK-371.1
title: Fix existing web-ui package formatting drift
status: Backlog
assignee: []
created_date: '2026-06-27 04:46'
labels:
  - web-ui
  - formatting
  - maintenance
milestone: 'm-15: Testing & Quality Assurance'
dependencies: []
references:
  - packages/web-ui/src/export/mod.rs
  - packages/web-ui/src/views/compliance.rs
priority: medium
ordinal: 0
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

While verifying TASK-371, `nix develop -c cargo fmt --manifest-path packages/web-ui/Cargo.toml -- --check` reported formatting diffs in unrelated files, including `packages/web-ui/src/export/mod.rs` and `packages/web-ui/src/views/compliance.rs`. This prevents package-wide web-ui rustfmt from being used as a clean verification gate for scoped UI changes.

## Desired Outcome

Bring the existing web-ui Rust sources back into rustfmt compliance without mixing the unrelated formatting cleanup into feature or bug-fix tasks.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 `nix develop -c cargo fmt --manifest-path packages/web-ui/Cargo.toml -- --check` passes for the web-ui package.
- [ ] #2 Formatting-only changes are separated from unrelated behavior changes.
<!-- AC:END -->
