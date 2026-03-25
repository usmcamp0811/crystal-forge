---
id: TASK-156
title: Add completed builds view to build queue UI
status: Done
assignee: []
created_date: '2026-03-02 04:41'
updated_date: '2026-03-24 20:45'
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
- [x] #1 The builds page includes a clear switcher between Active Queue and Completed Builds.
- [x] #2 The Completed Builds view shows successful and failed completed builds without removing the existing active queue view.
- [x] #3 Completed builds can be filtered by status and sorted by completion time at minimum.
- [x] #4 Completed build rows show system/hostname, environment where available, status, completion time, and duration.
- [x] #5 Existing active queue behavior continues to function unchanged after the new view is added.
- [x] #6 Local verification instructions cover both queue and completed-builds behavior.
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
LOCK: OpenCode-gpt5 on reckless in ~/code/crystal-forge/TASK-156-add-completed-builds-view

Implemented initial completed-builds view in `packages/web-ui/src/views/builds.rs` with:
- Active Queue / Completed Builds tab switcher
- Completed table with columns: system, environment, status, completion time, duration, commit
- Status filter (All / Complete / Failed)
- Completion time sort (newest/oldest)
- Active queue pane behavior kept unchanged under Active Queue tab

Extended build UI model in `packages/web-ui/src/components/builds/helpers.rs`:
- Added `environment`, `duration_secs`, and `completed_at` fields to `BuildItem`

Verification run:
- `nix develop -c cargo check --manifest-path packages/web-ui/Cargo.toml` ✅
- `nix develop -c rustfmt --edition 2021 --check packages/web-ui/src/views/builds.rs packages/web-ui/src/components/builds/helpers.rs` ✅

Note: repository-wide `cargo fmt -- --check` in this worktree reports unrelated pre-existing formatting drift in other files not touched by this task.

---

**MR Created**: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/183

Final commits:
1. c2fe201c - Initial completed builds feature
2. 3269df10 - Status badge CSS fix + web-ui check updates
3. 18e397bc - System name extraction from flake paths
4. 2dae4392 - Environment field addition (backend + frontend)
5. 4483d4a5 - Mock data fixes for CI

**MR Merged**: 2026-03-24

Additional features delivered beyond initial scope:
- Environment column properly wired from backend through to UI
- System name extraction helper for clean display (strips flake URI paths)
- Shared CSS classes for status badges (theme-ready)
- Web-UI check test coverage for new feature
- Screenshots captured and documented in MR

Task completed successfully. All acceptance criteria met.
<!-- SECTION:NOTES:END -->
