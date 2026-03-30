---
id: TASK-229
title: >-
  Show expected/current config store paths in Flakes commit details to resolve
  unknown deployment status
status: To Do
assignee: []
created_date: '2026-03-30 03:17'
updated_date: '2026-03-30 03:17'
labels:
  - flakes
  - ui
  - backend
  - deployment
  - sprint-ready
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem Statement
In the Flakes view, commit details currently show system configurations but not the resolved Nix store paths per configuration. Operators cannot compare those expected paths against what agents report as current (`/run/current-system`) from the same screen. This contributes to systems appearing as `unknown` even when the latest config has already been deployed.

## Goal
Update Flakes commit details so each system configuration includes path visibility that lets operators determine whether an agent-reported current path matches a known configuration from the processed commit, without requiring a successful build artifact first.

## Non-Goals
- No redesign of the full Flakes page layout beyond commit-details presentation.
- No change to agent payload schema.
- No replacement of existing build-complete `store_path` behavior; this task is visibility + matching UX/data wiring.
- No broad dashboard refactor.

## Scope
- Add commit-details UI for per-configuration paths (tab/section/panel pattern acceptable if consistent with existing design).
- Surface expected path values derived from processed commit/eval data.
- Surface agent-reported current path context in commit details when available for quick comparison.
- Ensure unknown/missing-path states are explicit.

## Architectural Constraints
- Keep business logic in backend query/service layers; UI components should only present mapped data.
- Reuse existing source-of-truth semantics for expected path vs built path vs current path.
- Preserve API model boundaries (DTOs mirror server models).

## Verification Plan
- Backend/API test coverage for commit-details payload including expected/current path fields and unknown cases.
- UI/component test (or integration test) proving commit details render paths per config and unknown states.
- End-to-end/feature check validating a processed (not yet built) commit can still display a known expected path match against current agent path.
- Targeted Nix dev checks for touched packages/modules.

## Impact Areas
- Flakes commit-details backend query/service path projection
- API models/handlers for Flakes commit details
- Dioxus Flakes view commit-details component(s)
- Any existing deployment-status helper used by commit detail rendering

## Risk Level
Medium-High (incorrect path mapping can mislead deployment confidence and operator actions).

## Dependencies
- Relies on expected-store-path persistence and matching semantics from TASK-225 being available in `dev` (now merged).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Commit details display, for each listed system configuration, the expected store path from processed commit/eval data (or an explicit unavailable state).
- [ ] #2 When an agent-reported current store path exists for a system, commit details show that current path and a deterministic match status against known expected path data.
- [ ] #3 A commit that is processed but not built still allows operators to see path-based known/unknown matching context for affected systems.
- [ ] #4 Unknown status is only shown when required path data is genuinely unavailable or non-matching, not merely because build output path is absent.
- [ ] #5 Backend/API and UI tests cover: matching path, non-matching path, and missing path cases.
- [ ] #6 The implementation is documented in task notes with precedence rules for matching (e.g., built path vs expected path) used by commit details.
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Moved Backlog -> To Do per explicit human sprint selection request in chat.

Task authored to Sprint-Ready quality: includes problem, goal, non-goals, constraints, verification plan, impact areas, risk, dependencies, and objective acceptance criteria.
<!-- SECTION:NOTES:END -->
