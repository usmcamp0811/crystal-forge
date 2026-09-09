---
id: TASK-410
title: Refactor Dashboard view UI/UX for exact parity with latest design examples
status: Done
assignee:
  - opencode-agent
created_date: '2026-05-31 15:14'
updated_date: '2026-09-02 02:41'
labels:
  - ui
  - dashboard
  - design-parity
  - strict-parity
  - loading-state
milestone: m-16
dependencies:
  - TASK-410.1
references:
  - /home/mcamp/code/crystal-forge/CrystalForgelatest
  - TASK-415
  - TASK-433.8
modified_files:
  - packages/web-ui/src/views/dashboard.rs
  - packages/web-ui/src/components/dashboard/cve_summary.rs
  - packages/web-ui/src/components/dashboard/operational.rs
  - packages/web-ui/src/components/dashboard/mod.rs
  - packages/web-ui/src/components/loading.rs
  - packages/web-ui/src/components/widget_grid.rs
  - packages/web-ui/assets/app.css
  - checks/web-ui/tests/integration-test.js
  - checks/web-ui/coverage-manifest.json
priority: high
ordinal: 22000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem
The current Dashboard view in the web UI diverges from the latest design examples in layout precision, spacing rhythm, typography hierarchy, visual states, and interaction details. This creates inconsistency and lowers confidence in the design system rollout.

## Goal
Implement a dashboard refactor that matches the latest design examples with strict visual and behavioral parity, including all loading-state behavior and spinner treatment shown in the source examples.

## Design Source (Authoritative)
- `/home/mcamp/code/crystal-forge/CrystalForgelatest`
- Includes Dashboard examples and related component references that define expected visuals, interactions, and loading states.

## Non-Goals
- No backend/API contract redesign unless absolutely required to render already-available data and tracked separately.
- No unrelated redesign in non-dashboard views.
- No broad component-library rewrite unless directly required for dashboard parity.
- No speculative feature additions absent from the design source.

## Execution Expectations (High-Discipline)
This task requires exacting attention to detail:
- Pixel-accurate layout, spacing, sizing, and alignment across breakpoints.
- Exact text hierarchy, copy, labels, and semantic emphasis from the design source.
- Exact visual states (default/hover/focus/active/disabled/loading/error) where shown.
- No “close enough” substitutions for core visual structure.

## Architectural Constraints
- Keep business logic out of view rendering.
- Keep state/data mapping separate from presentational components.
- Reuse existing design-system primitives where they already satisfy design parity.
- Maintain clear separation of API models, domain logic, infrastructure, and UI layers.

## Impact Areas
- `packages/web-ui/src/views/**` (dashboard-specific view modules)
- `packages/web-ui/src/components/**` (dashboard-specific or reused presentation components as needed)
- `checks/web-ui/**` (integration checks, assertions, screenshot evidence)
- Any route wiring that composes dashboard view surfaces

## Verification Plan
Tier 1 feature-level + targeted Tier 0 checks:
- `nix develop -c cargo check --manifest-path packages/web-ui/Cargo.toml --target wasm32-unknown-unknown`
- `nix build .#checks.x86_64-linux.web-ui`
- Ensure web-ui check captures updated Dashboard screenshots covering:
  - Initial loading state with spinner
  - Post-load default dashboard state
  - At least one responsive/mobile layout state
- Add/adjust assertions to verify key dashboard interactions and visible state transitions from loading → loaded.

## Risk Level
High (high-visibility screen with strict visual parity and responsive expectations).

## Dependencies
- Design examples remain accessible at `/home/mcamp/code/crystal-forge/CrystalForgelatest`.
- No conflicting active task heavily modifying same dashboard files during implementation.
- Existing web-ui test harness capable of capturing screenshots for the targeted states.

## Definition of Ready Notes
This task is intentionally strict and objective. Parity decisions must defer to the local design source when ambiguity appears.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Dashboard layout matches the authoritative design examples from `/home/mcamp/code/crystal-forge/CrystalForgelatest` at desktop and mobile breakpoints with pixel-accurate alignment, spacing, and sizing.
- [ ] #2 Dashboard typography hierarchy, labels, copy, and emphasis match the design examples exactly (no placeholder substitutions in primary UI text).
- [ ] #3 Visual styling matches design source for colors, radii, borders, shadows, icon sizing, and component density in all visible dashboard sections.
- [ ] #4 Interaction states shown in the design examples are implemented and validated (hover/focus/active/disabled/error where applicable).
- [ ] #5 Loading experience matches design source and includes the dashboard loading spinner behavior and presentation exactly as shown.
- [ ] #6 State transition from loading spinner state to loaded dashboard state is deterministic and covered by web-ui assertions.
- [ ] #7 No business logic is introduced into presentational components; state/data mapping remains separated from rendering.
- [ ] #8 No backend/API contract changes are introduced unless explicitly required and tracked via separate scoped task(s).
- [ ] #9 `nix develop -c cargo check --manifest-path packages/web-ui/Cargo.toml --target wasm32-unknown-unknown` passes.
- [ ] #10 `nix build .#checks.x86_64-linux.web-ui` passes and includes dashboard screenshot evidence for loading spinner state and loaded state.
- [ ] #11 web-ui check must include assertion-based validation for all updated dashboard interactions
- [ ] #12 web-ui check must include screenshots for all dashboard parity states
<!-- AC:END -->

## Definition of Done
<!-- DOD:BEGIN -->
- [ ] #1 Attach dashboard loading-state and loaded-state screenshot evidence from web-ui checks to MR.
- [ ] #2 Document any unavoidable visual deltas with rationale and explicit sign-off request (if exact parity cannot be achieved due to technical constraints).
<!-- DOD:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
## Approved execution breakdown

User-approved boundaries:
- TASK-410 owns shared dashboard/frontend parity, non-POA&M widgets, source-matched loading behavior, responsive behavior, and authoritative browser evidence.
- TASK-410.1 owns the missing real-data dashboard aggregate APIs (build/eval/activity/graph/cache) required by AC #8; no fabricated values.
- The active deployment-approvals/running-state-attestations task commonly referenced as TASK-415 exclusively owns Deploy Approvals and Attestation Trust server semantics and dashboard integration; TASK-410 will consume/validate the merged result rather than duplicate it. Its malformed backlog metadata prevents adding it as a formal dependency, so the boundary is recorded here and in References.
- TASK-433.8 exclusively owns POA&M Summary and Watchlist. TASK-410 will not add POA&M registry IDs, DTOs, widgets, API calls, mocks, tests, or layout migration.
- TASK-412 retains scanning/XCCDF/compliance/policy ownership; TASK-410 does not modify those surfaces.

## Implementation sequence

1. Replace the dashboard heartbeat-style spinner with the exact generic source spinner geometry, timing, accessibility semantics, and reduced-motion behavior. Gate initial dashboard rendering behind one primary page-loading state and preserve non-fabricated error behavior.
2. Align registry/default ordering and widget metadata for non-POA&M widgets that have real data on current `dev`, preserving saved layouts via additive versioned migration without inserting duplicates. Do not add approval/attestation widgets until their owning task's interfaces merge.
3. Implement current-API-backed widgets and pure adapters: CVE Summary from fleet stats (including exploited/fixable), Heartbeats from effective intervals, Top CVE-affected Systems with current authorization limits, Recent Commits from real timelines, Environments from real environment summaries, and Quick Actions. Keep data mapping outside presentational components.
4. Align customization interactions with source behavior: drag-end cleanup, exact control titles/icons/action labels, add/remove/reset/persistence behavior, and mobile usability.
5. Update authoritative web-ui coverage for deterministic source-matched loading, loading→loaded transition, populated desktop, responsive/mobile, customization interactions, error/no-fabricated-data, and every interaction changed in this phase. Update coverage metadata alongside step names.
6. Run targeted formatting/tests, required WASM cargo check, and `nix build .#checks.x86_64-linux.web-ui`; record exact outcomes and screenshot paths.
7. After TASK-410.1 and the approval/attestation owner merge, rebase and integrate/validate the remaining real-data widgets, then repeat full TASK-410 verification. POA&M remains excluded for TASK-433.8.

## Checkpoints and risks

- Current implementation can deliver steps 1–6 for current-API-backed widgets, but TASK-410 cannot satisfy complete authoritative widget content until TASK-410.1 and the approval/attestation work merge.
- Role-scoped product authorization takes precedence over the role-agnostic mock design and will be asserted explicitly.
- Source code (`DashboardView.jsx`, `data-dashboard.js`, `styles.css`, `Spinner.jsx`) is authoritative over stale checked-in screenshots.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
LOCK: opencode-agent on reckless in /home/mcamp/code/crystal-forge/TASK-410-dashboard-parity

Preflight: branch `TASK-410-dashboard-parity` at committed `dev` HEAD `24835e6c`; dedicated worktree verified clean. The `main` integration worktree is clean. Per user authorization, the three unrelated untracked files in the `dev` integration worktree are left untouched. Authoritative design source for this execution is the repository copy under `docs/design/CrystalForge`, per user direction. Intended scope is dashboard view/components/styles and authoritative web-ui assertions/screenshots only. Planned verification follows task AC: WASM cargo check and `checks.x86_64-linux.web-ui`, with targeted formatting/tests as applicable.

Research completed against `docs/design/CrystalForge`. TASK-342.2 already matches the authoritative 3/2/1-column grid, base card styling, page heading, customization shell, and Fleet Health. Remaining source drift is substantial: the design defaults 15 widgets while current product defaults 8–10; Build Queue, CVE Summary, Flake Git Graph, Deployment Timeline, and spinner internals differ; mobile and deterministic loading→loaded evidence are missing. Exact source widgets Attestation Trust, Deploy Approvals, POA&M Summary/Watchlist, and exploited-CVE metrics lack existing frontend contracts/data. Adding fabricated data would violate prior no-mock behavior; adding backend contracts would conflict with this task's frontend scope/AC #8 unless separately tracked. The source dashboard has no page loading/error branch; only generic `Spinner.jsx` defines exact spinner geometry/accessibility. Awaiting user scope decision before recording the implementation plan or writing application code.

Frontend phase progress: implemented the exact Spinner.jsx ring geometry (27% arc, 0.78s linear rotation, accessible status label, reduced-motion treatment) and a single primary page-loading gate; added deterministic drag-end cleanup and source control titles; added real-data CVE exploited/patchable, Heartbeats, Top CVE-affected Systems (admin-scoped), Recent Commits, and Environments widgets; aligned non-POA&M default ordering and added a versioned saved-layout migration; filtered unauthorized CVE actions. Added current-source loading→loaded, mobile one-column, real-data/error, default-order, and cancelled-drag assertions. Targeted WASM cargo check and heartbeat unit test passed. Full web-ui Nix check pending. Remaining source widgets are blocked on TASK-410.1 and the active approval/attestation owner; POA&M remains exclusively TASK-433.8.

Current-phase verification:
- PASS: `node --check checks/web-ui/tests/integration-test.js`.
- PASS: changed-file Rust formatting via `nix develop -c rustfmt --edition 2024 --check ...` (the package-wide cargo fmt check is currently blocked by unrelated pre-existing formatting drift in policy/compliance/environment files).
- PASS: `nix develop -c cargo check --manifest-path packages/web-ui/Cargo.toml --target wasm32-unknown-unknown`.
- PASS: `nix develop -c cargo test --manifest-path packages/web-ui/Cargo.toml heartbeat_rollup_uses_each_systems_effective_interval` (1 passed).
- PASS (exit 0): `nix build .#checks.x86_64-linux.web-ui --out-link /tmp/cf-task410-web-ui-check`.
- PASS with all selected results `ok: true`: focused dashboard runs covering loading spinner, loading→loaded, mobile loaded, recent-deployment optional-layout regression, Fleet Health, API error/no-fabricated data, populated widget parity/default ordering/cancelled drag, and optional Pipeline Readiness. Final focused artifacts are under `/tmp/cf-task410-dashboard-final/screenshots`; earlier loading/mobile/error evidence is in the Nix store paths recorded during this session.

Verification caveat: the full web-ui derivation currently exits 0 even when unrelated browser steps in `results.json` report `ok: false`. TASK-434 was created to make those failures propagate. The TASK-410 dashboard steps were rerun focused after fixes and all passed with dark/light screenshots. TASK-410 remains In Progress because TASK-410.1 and the approval/attestation owner must merge before remaining real-data widgets can be integrated; no POA&M work was duplicated.
<!-- SECTION:NOTES:END -->
