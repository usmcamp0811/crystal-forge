---
id: TASK-364
title: 'Builders: bring Builders view to CrystalForgelatest parity'
status: Review
assignee:
  - '@opencode-agent'
created_date: '2026-06-20 02:07'
updated_date: '2026-06-20 04:25'
labels:
  - design-parity
  - builders
  - web-ui
  - parity
milestone: 'm-19: Design Parity Existing Surfaces'
dependencies: []
references:
  - >-
    /home/mcamp/code/crystal-forge/CrystalForgelatest/components/BuildersView.jsx
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/283'
documentation:
  - design/doc-8 - CrystalForgelatest-UI-Parity-Matrix-TASK-328.md
  - design/doc-14 - Parity-execution-playbook-agent-proof.md
modified_files:
  - packages/web-ui/src/views/builders.rs
  - packages/web-ui/src/components/builders/add_builder_modal.rs
  - packages/web-ui/src/components/builders/builder_card.rs
  - packages/web-ui/src/components/builders/builder_row.rs
  - packages/web-ui/src/components/builders/builders_list.rs
  - checks/web-ui/tests/integration-test.js
priority: high
ordinal: 1785
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem Statement
The Builders view needs a dedicated design-parity pass against `CrystalForgelatest/components/BuildersView.jsx`. Existing parity work covers Builds, Evaluations, CVEs, Systems, Flakes, Environments, and related surfaces, but no sprint-ready task currently tracks `packages/web-ui/src/views/builders.rs` parity.

## Goal
Bring the Dioxus Builders view to parity with the CrystalForgelatest Builders reference while preserving real API-backed data flow and existing authorization boundaries.

## Explicit Non-Goals
- Do not change Builds queue behavior; Builds view parity is tracked separately.
- Do not change builder backend scheduling, execution, cancellation, or heartbeat semantics unless required to expose already-existing data safely.
- Do not introduce fake/sample business data into production views.
- Do not refactor unrelated builder, cache, or build queue modules.

## Architectural Constraints
- UI must consume real API data through existing client/model patterns.
- No business logic in UI views; keep formatting/presentation in view helpers only.
- Preserve authorization behavior for builder mutation controls.
- Follow existing Dioxus component/view patterns and design tokens.
- Any API/model change must be minimal and documented in the implementation notes.

## Impact Areas
- `packages/web-ui/src/views/builders.rs`
- `packages/web-ui/src/components/builders/`
- `packages/web-ui/src/api/client.rs`
- `packages/web-ui/src/api/models.rs`
- `checks/web-ui/tests/integration-test.js`
- Reference: `CrystalForgelatest/components/BuildersView.jsx`

## Risk Level
Medium. The work is UI-focused but touches an operational surface with role-gated controls, live builder state, and existing web-ui check coverage.

## Dependencies
None known. Confirm current `dev` branch and existing web-ui check behavior before implementation.

## Verification Plan
- `nix develop -c cargo fmt --manifest-path packages/web-ui/Cargo.toml -- --check`
- `nix develop -c cargo check --manifest-path packages/web-ui/Cargo.toml --target wasm32-unknown-unknown`
- `nix build .#checks.x86_64-linux.web-ui -L`
- Update/reuse Builders check steps, especially `11b-builders` and any existing Builders modal/action steps, so the parity result is captured in screenshots.

## Implementation Notes For Future Agent
This task is sprint-selected and ready for execution, but it is not implemented by this task creation step. Follow the repository worktree/backlog workflow before changing code.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Builders view layout, density, cards/table structure, status treatments, and action surfaces match `CrystalForgelatest/components/BuildersView.jsx` within Dioxus constraints
- [x] #2 Builders view uses real API-backed data and does not introduce fabricated production data or UI-only business state
- [x] #3 Builder mutation/action controls preserve existing authorization behavior and do not expose operator/admin actions to unauthorized users
- [x] #4 Loading, empty, and error states match the reference design patterns and do not silently fall back to mock data
- [x] #5 Existing and/or updated web-ui check steps capture the Builders parity surface in screenshots
- [x] #6 Targeted formatting and web-ui compile checks pass
- [x] #7 `nix build .#checks.x86_64-linux.web-ui -L` passes or any unrelated existing failures are explicitly documented with the Builders steps passing
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
MR: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/283
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
MR: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/283

Summary:
- Aligned Builders list/table/card surface with the CrystalForgelatest reference while preserving real API data flow.
- Removed fabricated 24h/load metric values and displays unavailable state when BuilderSummary lacks those metrics.
- Corrected builder registration public-key guidance to base64 Ed25519 format.
- Hid builder mutation controls from non-admin users in both BuildersView and legacy BuildersList, matching backend admin-only builder management endpoints.
- Updated web-ui fixtures/checks so Builders list and edit modal screenshots are captured deterministically.

Verification:
- `nix develop -c cargo fmt --manifest-path packages/web-ui/Cargo.toml -- --check` passed.
- `nix develop -c cargo check --manifest-path packages/web-ui/Cargo.toml --target wasm32-unknown-unknown` passed with existing warnings.
- `node --check checks/web-ui/tests/integration-test.js` passed.
- `nix build .#checks.x86_64-linux.web-ui -L` passed with exit status 0; Builders steps `[OK] 11b-builders` and `[OK] 11c-builders-edit-modal` were captured.
<!-- SECTION:FINAL_SUMMARY:END -->
