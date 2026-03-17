---
id: TASK-156
title: Add completed builds view to build queue UI
status: To Do
assignee: []
created_date: '2026-03-02 04:41'
updated_date: '2026-03-17 00:12'
labels:
  - ui
  - build-queue
  - enhancement
  - feature
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem
The current build queue UI only shows queued and actively building jobs. Operators have no way to inspect recently completed builds, failed builds, or historical execution details from the existing builds page.

## Goal
Add a completed-builds view to the builds UI so users can inspect historical build results alongside the active queue without losing the current queue workflow.

## Non-Goals
- This task does NOT redesign the entire builds page information architecture.
- This task does NOT introduce analytics dashboards, charts, or timeline visualizations beyond the initial completed-builds view.
- This task does NOT add export/reporting features.
- This task does NOT change build execution behavior or queue semantics.

## Scope
1. Add a view switcher or tabs on the builds page for Active Queue vs Completed Builds.
2. Render completed builds in a dense historical view appropriate for inspection.
3. Support practical filtering/sorting for completed results.
4. Preserve existing active queue behavior.

## Architectural Constraints
- Prefer a table-based completed-builds view for the first iteration.
- Keep queue behavior and completed-history rendering separate at the UI state level.
- Reuse existing build status models and styling patterns where possible.
- Avoid introducing business logic into presentational components.
- Follow existing Dioxus view/component patterns in `packages/web-ui/src/views/builds.rs` and related components.

## Impact Areas
- `packages/web-ui/src/views/builds.rs`
- `packages/web-ui/src/components/builds/`
- API client/model files if a completed-builds fetch path or params need extension

## Risk Level
Medium — user-facing view expansion in a complex page, but expected to be additive if queue behavior remains isolated.

## Verification Plan
- Tier 0:
  - `nix develop -c cargo fmt -- --check`
  - `nix develop -c cargo check --manifest-path packages/web-ui/Cargo.toml`
  - targeted frontend tests if present
- Tier 1:
  - Run the web UI and verify switching between Active Queue and Completed Builds
  - Confirm completed builds can be filtered/sorted as defined
  - Confirm active queue behavior is unchanged
- Tier 2:
  - `nix flake check` not required unless API/model or wider package integration forces it
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The builds page includes a clear switcher between Active Queue and Completed Builds.
- [ ] #2 The Completed Builds view shows successful and failed completed builds without removing the existing active queue view.
- [ ] #3 Completed builds can be filtered by status and sorted by completion time at minimum.
- [ ] #4 Completed build rows show system/hostname, environment where available, status, completion time, and duration.
- [ ] #5 Existing active queue behavior continues to function unchanged after the new view is added.
- [ ] #6 Local verification instructions cover both queue and completed-builds behavior.
<!-- AC:END -->
