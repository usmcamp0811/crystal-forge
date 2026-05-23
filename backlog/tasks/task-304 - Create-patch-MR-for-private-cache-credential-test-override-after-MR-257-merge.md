---
id: TASK-304
title: Create patch MR for private cache credential-test override after MR 257 merge
status: Review
assignee: []
created_date: '2026-05-23 03:44'
updated_date: '2026-05-23 03:44'
status: Backlog
assignee: []
created_date: '2026-05-23 03:44'
updated_date: '2026-05-23 03:44'
updated_date: '2026-05-23 03:45'
updated_date: '2026-05-23 03:51'
labels:
  - patch
  - caches
  - backend
  - nixos
milestone: UI/UX Design System
dependencies: []
references:
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/257'
  - 5def2c2e
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/260'
modified_files:
  - packages/default/src/handlers/api/caches.rs
  - packages/default/src/config/server.rs
  - modules/nixos/crystal-forge/default.nix
priority: high
ordinal: 4900
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem
MR #257 was merged before commit `5def2c2e` (opt-in private/internal cache credential-test override) was included, so the fix is not in `dev`.

## Goal
Create a focused patch MR that carries commit `5def2c2e` (or equivalent minimal changes) onto a new branch from `dev`.

## Desired Outcome
A new merge request is opened with only the missing patch changes:
- server config toggle `allow_private_cache_test_targets` (default false)
- handler wiring in cache credential test endpoint
- NixOS module option + config emission
- targeted tests proving default deny / opt-in allow behavior
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 A new branch from dev contains only the missing patch changes from commit 5def2c2e (or exact equivalent).
- [x] #2 `nix develop -c env SQLX_OFFLINE=true cargo check --manifest-path packages/default/Cargo.toml -p crystal-forge` passes.
- [x] #3 `nix develop -c env SQLX_OFFLINE=true cargo test --manifest-path packages/default/Cargo.toml --lib handlers::api::caches` passes.
- [x] #4 A new MR is opened in GitLab with task reference and verification results.
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Patch branch: TASK-304-private-cache-test-patch

Cherry-picked commit: c57ba32e

MR opened: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/260
<!-- SECTION:NOTES:END -->
