---
id: TASK-410
title: Refactor Dashboard view UI/UX for exact parity with latest design examples
status: In Progress
assignee:
  - opencode-agent
created_date: '2026-05-31 15:14'
updated_date: '2026-08-23 19:59'
labels:
  - ui
  - dashboard
  - design-parity
  - strict-parity
  - loading-state
milestone: m-16
dependencies: []
references:
  - /home/mcamp/code/crystal-forge/CrystalForgelatest
modified_files:
  - packages/web-ui/src/views/
  - packages/web-ui/src/components/
  - checks/web-ui/
priority: high
ordinal: 20000
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

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
LOCK: opencode-agent on reckless in /home/mcamp/code/crystal-forge/TASK-410-dashboard-parity

Preflight: branch `TASK-410-dashboard-parity` at committed `dev` HEAD `24835e6c`; dedicated worktree verified clean. The `main` integration worktree is clean. Per user authorization, the three unrelated untracked files in the `dev` integration worktree are left untouched. Authoritative design source for this execution is the repository copy under `docs/design/CrystalForge`, per user direction. Intended scope is dashboard view/components/styles and authoritative web-ui assertions/screenshots only. Planned verification follows task AC: WASM cargo check and `checks.x86_64-linux.web-ui`, with targeted formatting/tests as applicable.

Research completed against `docs/design/CrystalForge`. TASK-342.2 already matches the authoritative 3/2/1-column grid, base card styling, page heading, customization shell, and Fleet Health. Remaining source drift is substantial: the design defaults 15 widgets while current product defaults 8–10; Build Queue, CVE Summary, Flake Git Graph, Deployment Timeline, and spinner internals differ; mobile and deterministic loading→loaded evidence are missing. Exact source widgets Attestation Trust, Deploy Approvals, POA&M Summary/Watchlist, and exploited-CVE metrics lack existing frontend contracts/data. Adding fabricated data would violate prior no-mock behavior; adding backend contracts would conflict with this task's frontend scope/AC #8 unless separately tracked. The source dashboard has no page loading/error branch; only generic `Spinner.jsx` defines exact spinner geometry/accessibility. Awaiting user scope decision before recording the implementation plan or writing application code.
<!-- SECTION:NOTES:END -->
