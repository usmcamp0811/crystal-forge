---
id: TASK-334
title: >-
  Create Compliance view with exact CrystalForgelatest parity and backend-backed
  data
status: Backlog
assignee: []
created_date: '2026-05-31 16:02'
updated_date: '2026-06-21 01:57'
labels:
  - design-parity
  - compliance
  - web-ui
  - api-integration
milestone: m-20
dependencies:
  - TASK-328
  - TASK-329
  - TASK-332
  - TASK-333
references:
  - /home/mcamp/code/crystal-forge/CrystalForgelatest
  - TASK-317
  - TASK-319
documentation:
  - design/doc-10 - CrystalForgelatest-parity-execution-plan.md
  - design/doc-11 - CrystalForgelatest-design-source-index.md
modified_files:
  - packages/web-ui/src/views/compliance.rs
  - checks/web-ui
priority: high
ordinal: 1660
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem
The Compliance view is not yet implemented to match the CrystalForgelatest design reference. The existing compliance UI from TASK-319 provides a basic backend-backed flow (Bundle → Control → Evidence → Waiver), but it has not received the final design-parity pass. This leaves visual gaps, inconsistent states, and missing interactions that block acceptance.

## Goal
Implement the Compliance view so it matches the CrystalForgelatest reference at `ComplianceView.jsx` pixel-for-pixel, renders authoritative backend data for all primary states, and is covered by web-ui screenshot/assertion checks.

## Non-Goals
- Broad redesign of unrelated views
- Speculative compliance features outside the reference design (e.g., bulk editing, custom compliance scoring, external GRC integrations)
- Replacing the compliance MVP/evaluator (TASK-317) with UI-only placeholders
- Building backend features not already established by the evaluator/domain work (TASK-312–TASK-317) and UI skeleton (TASK-319)
- The Compliance sidebar surface (tracked separately by TASK-344)

## Exact Scope
1. **Bundle catalog sidebar**: list of compliance bundles with selection state matching the reference
2. **Bundle header**: bundle name, description, metadata, overall compliance score rendering
3. **Score/stat strip**: pass/warn/fail/waiver counts, compliant hosts, overall score percentage per the stat-strip pattern
4. **System-level per-bundle view**: table or card list of applicable systems with per-system control rollup (pass/warn/fail/waiver/total), filterable by all/fail/warn/clean
5. **Drill-in system control detail**: per-system control breakdown showing individual control status, mapped policies, evidence actions, and waiver state (backed by TASK-319 evaluator data)
6. **Export evidence button**: wired to compliance export endpoints (TASK-318)
7. **New bundle creation**: minimal create flow per the reference
8. **States**: loading, empty (no bundles/applicable systems), error, and populated states rendered per the reference with no production-path mock fallback
9. **web-ui check**: screenshot coverage and assertion-based validation for all major Compliance interactions and state transitions

## Architectural Constraints
- No business logic in UI views; frontend renders DTOs only
- Status rendering must preserve layered assertions (pass/warn/fail/waiver) — no lossy single-badge simplification
- Reuse existing Crystal Forge layout/navigation conventions (page-head, stat-strip, cards/table patterns from TASK-353/TASK-358)
- New compliance-specific components go in `packages/web-ui/src/components/compliance/`
- Any temporary mock data for parity gaps without current backend support must be clearly commented and tracked by a follow-up Backlog task
- Export evidence buttons reference TASK-318 export endpoints; wire as available, leave clearly marked placeholders otherwise
- Respect the existing compliance information architecture from TASK-319 (Bundle → Control → Evidence → Waiver flow)

## Verification Plan
- `nix develop -c cargo fmt -- --check`
- `nix develop -c cargo clippy -- -D warnings`
- `nix develop -c cargo check --manifest-path packages/default/Cargo.toml --all-targets`
- `nix develop -c cargo check --manifest-path packages/web-ui/Cargo.toml --all-targets`
- `nix develop -c cargo test --manifest-path packages/default/Cargo.toml --lib queries compliance`
- `nix develop -c cargo test --manifest-path packages/web-ui/Cargo.toml --bin crystal-forge-ui compliance`
- `nix develop -c cargo sqlx prepare --check` (if schema changes)
- `nix build .#checks.x86_64-linux.web-ui` — must pass and produce Compliance view screenshots
- Visual diff against CrystalForgelatest `ComplianceView.jsx` for bundle catalog, score strip, system table, drill-in detail, and export/new-bundle modals

## Impact Areas
- `packages/web-ui/src/views/compliance.rs`
- `packages/web-ui/src/components/compliance/` (new sub-components for bundle catalog, score strip, system table, drill-in detail)
- `packages/web-ui/assets/app.css` (compliance-specific styles)
- `packages/web-ui/src/api/models.rs` (DTOs if extended)
- `packages/web-ui/src/api/client.rs` (API calls if wiring new endpoints)
- `checks/web-ui/tests/integration-test.js` (screenshot + assertion coverage)
- `packages/default/src/handlers/api/compliance.rs` (API changes if needed)
- `packages/default/src/queries/compliance.rs` (query changes if needed)

## Risk Level
Medium — primarily UI component work building on the established TASK-319 compliance information architecture and TASK-312–TASK-317 evaluator/domain work. Backend changes are moderate if export/new-bundle endpoints require extension. The task is sequenced after the evaluator and UI skeleton are complete, reducing integration risk.

## Dependencies
- TASK-328 — CrystalForgelatest parity spec (Done)
- TASK-329 — Foundation shell/tokens/topbar/sidebar parity (Done)
- TASK-332 — Align shared backend API contracts (Done)
- TASK-333 — (foundation gap)
- TASK-317 — Compliance evaluator service (establishes authoritative backend data)
- TASK-319 — Compliance UI skeleton / information architecture (establishes backend-backed Bundle→Control→Evidence→Waiver flow)
- TASK-318 — Compliance export endpoints (for export evidence button)
- CrystalForgelatest reference: `/home/mcamp/code/crystal-forge/CrystalForgelatest/components/ComplianceView.jsx`
- Design parity matrix: `design/doc-8 - CrystalForgelatest-UI-Parity-Matrix-TASK-328.md`
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 #1 Bundle catalog sidebar lists compliance bundles with selection state matching CrystalForgelatest reference on desktop
- [ ] #2 #2 Bundle header displays name, description, metadata, and overall compliance score per the reference
- [ ] #3 #3 Score/stat strip renders pass/warn/fail/waiver counts, compliant hosts, and overall score percentage
- [ ] #4 #4 System-level per-bundle view renders applicable systems with per-system control rollup, filterable by all/fail/warn/clean
- [ ] #5 #5 Drill-in system control detail shows individual control status, mapped policies, evidence actions, and waiver state
- [ ] #6 #6 Export evidence button is wired to compliance export endpoints (TASK-318) or marked as placeholder if endpoint unavailable
- [ ] #7 #7 New bundle creation flow matches the reference interaction
- [ ] #8 #8 Loading, empty, error, and populated states are styled and behaved per the reference with no production-path mock fallback (except authorized temporary placeholders with follow-up tracking)
- [ ] #9 #9 All primary displayed values are sourced from authoritative backend APIs in production paths
- [ ] #10 #10 checks/web-ui captures screenshot evidence and behavior assertions for the full Compliance surface (bundle catalog, score strip, system table, drill-in detail, export modal, states)
- [ ] #11 #11 A human reviewer can compare the implemented Compliance surface against the CrystalForgelatest reference and find no remaining material parity gaps
<!-- AC:END -->



## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Final compliance surface polish task; do not start until compliance evaluator outputs and baseline backend-backed compliance flow exist.
<!-- SECTION:NOTES:END -->
