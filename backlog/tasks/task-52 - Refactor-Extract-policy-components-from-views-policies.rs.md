---
id: TASK-52
title: 'Refactor: Extract policy components from views/policies.rs'
status: Review
assignee:
  - Claude Opus 4.5
created_date: '2026-02-18 02:46'
updated_date: '2026-02-21 03:28'
labels:
  - refactoring
  - web-ui
  - policy
milestone: m-13
dependencies: []
priority: low
ordinal: 2000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem Statement
`views/policies.rs` contains multiple policy UI responsibilities that were intended to live in reusable components under `components/policy/`. The policy component module is still effectively a placeholder, which keeps policy rendering and editing concerns tightly coupled to the view and makes future policy UI changes harder to test and reuse.

## Goal
Extract policy-specific UI blocks from `views/policies.rs` into reusable components in `components/policy/` and update imports/exports so the view composes those components instead of owning their implementation details.

## Non-Goals
- No behavior redesign of policy workflows, validation rules, or API contracts.
- No visual redesign beyond parity-preserving extraction adjustments.
- No backend or database changes.

## Architectural Constraints
- Keep business logic out of UI views; `views/policies.rs` should orchestrate and compose only.
- Keep reusable UI in `components/policy/` with clear boundaries and minimal coupling.
- Preserve existing repository patterns for component module exports and naming.

## Verification Plan
- `nix develop -c cargo check` (web-ui package)
- `nix develop -c cargo test` (web-ui package; targeted if tests are present)
- `nix build .#checks.x86_64-linux.web-ui`
- Manual smoke test: open policies view, render policy cards, open/edit modal flow, and confirm no regressions.

## Impact Areas
- `packages/web-ui/src/views/policies.rs`
- `packages/web-ui/src/components/policy/`
- `packages/web-ui/src/components/mod.rs` (if exports are adjusted)

## Risk Level
Low to Medium (UI refactor with potential wiring regressions if props/state ownership changes).

## Dependencies
None. Task may start immediately when selected for execution.
<!-- SECTION:DESCRIPTION:END -->

<!-- AC:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 `PolicyCard` and `PolicyEditorModal` are extracted into dedicated files under `packages/web-ui/src/components/policy/`.
- [ ] #2 `packages/web-ui/src/components/policy/mod.rs` exports extracted components and has no placeholder TODO entries for those exports.
- [ ] #3 `packages/web-ui/src/views/policies.rs` composes extracted components instead of defining extracted UI blocks inline.
- [ ] #4 Policy list rendering and policy edit modal interactions preserve existing behavior (open, cancel, submit).
- [ ] #5 `nix build .#checks.x86_64-linux.web-ui` completes successfully.
- [ ] #6 Manual policies-page smoke test passes with no regressions observed.
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
LOCK: gpt-5.3-codex on gray in /home/mcamp/code/crystal-forge/TASK-52-extract-policy-components

Extracted PolicyCard and PolicyEditorModal into packages/web-ui/src/components/policy/ with shared types module.

Verification executed: rustfmt (touched files), cargo check (web-ui), cargo test (web-ui), nix build .#checks.x86_64-linux.web-ui - all passed.

MR: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/115
<!-- SECTION:NOTES:END -->
