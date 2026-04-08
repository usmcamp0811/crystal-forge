---
id: TASK-196
title: >-
  Dashboard pipeline readiness widget doesn't support scrolling for multiple
  errors/warnings
status: Review
assignee: []
created_date: '2026-03-19 12:36'
updated_date: '2026-04-07 03:31'
labels:
  - bug
  - ui
  - dashboard
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem Statement
The dashboard Pipeline Readiness widget does not support scrolling for long lists of warnings/errors, so items overflow and become inaccessible.

## Goal
Add bounded vertical scrolling to the Pipeline Readiness issue list so all warnings/errors are visible without breaking card layout.

## Non-Goals
- No change to readiness scoring logic or backend data contracts.
- No policy rule changes.
- No dashboard-wide style redesign.

## Architectural Constraints
- Keep change in web-ui presentation layer only.
- Preserve current data model contracts and component boundaries.
- Match existing dashboard UX conventions for scrollable card content.

## Verification Plan
- Run targeted web-ui checks/build for changed component(s).
- Validate rendering with multiple readiness errors/warnings.
- Confirm no overflow/regression in adjacent dashboard widgets.

## Impact Areas
- Pipeline readiness dashboard component(s) under `packages/web-ui/src/components/dashboard/`
- Relevant CSS in `packages/web-ui/assets/app.css` (if needed)

## Risk Level
Low to Medium: UI overflow/scroll behavior can affect card height consistency.

## Dependencies
None.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Given multiple readiness errors/warnings beyond available card height, users can scroll to view all items.
- [x] #2 Readiness items stay within widget boundaries without visual overflow.
- [x] #3 Users receive a visible cue that content is scrollable (native scrollbar or equivalent).
- [x] #4 Dashboard layout remains stable with no clipping/overlap regressions in neighboring cards.
- [x] #5 Targeted web-ui verification passes for modified files.
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
LOCK: opencode-gpt-5.3-codex on reckless in /home/mcamp/code/crystal-forge/TASK-196-pipeline-readiness-scroll

MR: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/215

Implemented bounded scroll behavior for Pipeline Readiness issues in `packages/web-ui/src/views/dashboard.rs` via dedicated scroll region (`data-testid='pipeline-readiness-scroll'`) and explicit overflow containment.

Added regression assertion step `06x-pipeline-readiness-scroll` in `checks/web-ui/tests/integration-test.js` using large mocked config-health issue sets; verifies alert count and scroll activation.

Updated `checks/web-ui/default.nix` critical checks for ci_fast and full profiles to require `06x-pipeline-readiness-scroll`.

Verification run: `nix develop -c bash -lc "cd packages/web-ui && cargo check"` (pass), `nix develop -c bash -lc "cd packages/web-ui && rustfmt --edition 2024 --check src/views/dashboard.rs"` (pass), `node --check checks/web-ui/tests/integration-test.js` (pass), `nix build .#checks.x86_64-linux.web-ui` (pass).
<!-- SECTION:NOTES:END -->
