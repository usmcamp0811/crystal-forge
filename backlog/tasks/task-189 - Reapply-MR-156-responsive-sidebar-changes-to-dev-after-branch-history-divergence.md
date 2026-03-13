---
id: TASK-189
title: >-
  Reapply MR !156 responsive sidebar changes to dev after branch history
  divergence
status: Review
assignee: []
created_date: '2026-03-13 02:17'
updated_date: '2026-03-13 02:29'
labels:
  - frontend
  - recovery
  - git
  - web-ui
dependencies: []
references:
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/156'
  - packages/web-ui/src/components/layout/app_shell.rs
  - packages/web-ui/src/components/layout/sidebar.rs
  - packages/web-ui/src/components/layout/topbar.rs
  - packages/web-ui/assets/app.css
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

Merge Request `!156` (responsive sidebar navigation) is marked merged in GitLab, but its merge commit (`af5fc075`) is not reachable from the current `origin/dev` tip (`b4f7220d`). The responsive sidebar changes are therefore missing from `dev` despite the MR merge record.

## Goal

Safely reapply the functional changes from MR `!156` onto current `dev` and open a new MR so the responsive sidebar behavior is restored in branch history.

## Non-Goals

- Do not rewrite `dev` history.
- Do not force-push integration branches.
- Do not add new sidebar features beyond what MR `!156` already introduced.

## Scope

- Create a recovery branch from current `dev`.
- Reapply MR `!156` changes (prefer cherry-pick of merge commit with correct parent; resolve conflicts if needed).
- Validate frontend builds/checks required by task.
- Open a new MR targeting `dev` with clear recovery rationale.

## Architectural Constraints

- Recovery must preserve current `dev` commits and reintroduce missing sidebar changes additively.
- Follow existing responsive sidebar behavior defined in MR `!156` summary and files.
- Keep scope limited to restoration; no opportunistic refactors.

## Verification Plan

- Tier 0:
  - `nix develop -c cargo fmt -- --check`
  - `nix develop -c cargo check`
- Tier 1:
  - `nix build .#checks.x86_64-linux.web-ui`
  - Manual sanity check of sidebar behavior across desktop/tablet/mobile.
- Tier 2:
  - Not required unless additional cross-package changes are introduced.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 A recovery branch is created from current `dev` and contains the reapplication of MR `!156` changes.
- [x] #2 The resulting branch includes the responsive sidebar behavior originally delivered by MR `!156` (desktop full sidebar, narrow/tablet collapsed behavior, mobile drawer access).
- [x] #3 No force-push or integration-branch history rewrite is performed.
- [x] #4 `cargo fmt -- --check` and `cargo check` pass in the Nix dev environment.
- [x] #5 `nix build .#checks.x86_64-linux.web-ui` succeeds.
- [x] #6 A new MR targeting `dev` is opened with explanation that this is a recovery reapply of missing `!156` content.
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
LOCK: codex on reckless in ~/code/crystal-forge/TASK-189-reapply-mr-156-sidebar

Recovery branch created from current origin/dev: `TASK-189-reapply-mr-156-sidebar`.

Reapplied MR !156 by cherry-picking merge content onto current dev as commit `bedda750`.

Conflict handled: kept current backlog task file variant for TASK-158; code/UI changes from !156 retained.

Verification executed:

- `nix develop -c cargo fmt --manifest-path packages/web-ui/Cargo.toml -- --check` ✅

- `nix develop -c cargo check --manifest-path packages/web-ui/Cargo.toml` ✅

- `nix build .#checks.x86_64-linux.web-ui` ✅

MR: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/160
<!-- SECTION:NOTES:END -->
