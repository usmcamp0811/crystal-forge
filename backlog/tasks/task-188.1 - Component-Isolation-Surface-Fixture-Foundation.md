---
id: TASK-188.1
title: Component Isolation Surface + Fixture Foundation
status: In Progress
assignee: []
created_date: '2026-03-13 01:51'
updated_date: '2026-03-16 03:23'
labels:
  - frontend
  - ux
  - architecture
dependencies: []
references:
  - packages/web-ui/src/views/style_guide.rs
  - packages/web-ui/src/routes.rs
  - packages/web-ui/src/components/
  - packages/web-ui/src/theme/
documentation:
  - docs/specs/01-frontend-views.md
  - docs/architecture.md
parent_task_id: TASK-188
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

Component isolation work cannot scale unless there is a stable foundation: a discoverable in-repo showcase surface, shared demo conventions, and a reusable fixture strategy. Without this, each contributor creates ad-hoc demos and inconsistent patterns.

## Goal

Create the foundational infrastructure for isolation-driven UI development in Crystal Forge by establishing the showcase surface and shared fixture architecture.

## Non-Goals

- This task does NOT migrate all target components into isolation demos.
- This task does NOT define the full governance/process documentation (handled by another child task).
- This task does NOT redesign component visuals.

## Scope

1. Define and implement the canonical isolation surface (expand `/style-guide` or add dedicated showcase route).
2. Establish module structure for isolated demo entries (organized by primitives/composites/page-widgets).
3. Add shared typed fixture builders/helpers for deterministic mock data.
4. Add baseline showcase shell patterns: sectioning, state matrix container, responsive preview wrappers.
5. Ensure the isolation surface is discoverable from navigation and/or contributor docs.

## Architectural Constraints

- Prefer extending existing `style_guide.rs` and existing route patterns before introducing new tooling.
- Keep demos deterministic and local (no external dependency on hosted storybook-like platform).
- Use explicit prop-driven rendering for showcased components.
- Fixture data must be reusable and centralized.
- Maintain separation of concerns: showcase shell and fixtures must not include domain mutation logic.

## Impact Areas

- `packages/web-ui/src/views/style_guide.rs` (or new showcase view)
- `packages/web-ui/src/routes.rs`
- `packages/web-ui/src/components/` (showcase support wrappers if needed)
- `packages/web-ui/src/` fixture module location

## Risk Level

Medium — foundational structural changes; additive but can impact developer workflow if poorly organized.

## Verification Plan

- Tier 0:
  - `nix develop -c cargo fmt -- --check`
  - `nix develop -c cargo clippy -- -D warnings`
  - `nix develop -c cargo test`
- Tier 1:
  - Run web UI and verify isolation surface route loads.
  - Verify fixture-based demos render deterministically across reloads.
  - Verify responsive preview wrappers function at mobile + desktop widths.
- Tier 2:
  - `nix develop -c nix flake check`
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A canonical in-repo component isolation surface is implemented and routed (expanded `/style-guide` or dedicated showcase route).
- [ ] #2 The isolation surface has a structured navigation taxonomy for at least: primitives, composites, and page-widgets.
- [ ] #3 A shared typed fixture strategy is implemented in a centralized location and used by showcase demos.
- [ ] #4 A reusable state-matrix shell/pattern exists to display component states consistently.
- [ ] #5 A reusable responsive preview wrapper exists to test at least mobile and desktop breakpoints.
- [ ] #6 The isolation surface is discoverable from UI navigation or clearly linked from contributor-facing docs.
- [ ] #7 The isolation foundation compiles and runs without introducing regressions to existing routes/views.
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
LOCK: codex on reckless in ~/code/crystal-forge/TASK-188.1-component-isolation-foundation

Implementation started on branch `TASK-188.1-component-isolation-foundation` in dedicated worktree.

Initial foundation delivered: new `showcase` module (`fixtures.rs`, `shell.rs`), `main.rs` module wiring, and `StyleGuideView` refactored into taxonomy-based isolation surface sections.

Nix/Git tracking fix applied: staged new `src/showcase/*` files so Nix builds include them.

Verification run:

- `nix develop -c cargo check --manifest-path packages/web-ui/Cargo.toml` ✅

- `nix build .#checks.x86_64-linux.web-ui` ✅

Enhanced showcase foundation with comprehensive fixture strategy and interactive component demos

Added typed fixtures: SystemFixture, FlakeFixture, BuildFixture with realistic demo data

Expanded fixture collections: 4 stat cards, 4 timeline items, 4 systems, 2 flakes, 3 builds

Added standard responsive breakpoint constants: MOBILE_WIDTH, TABLET_WIDTH, DESKTOP_WIDTH, WIDE_WIDTH

Added new shell components: ResponsiveGrid, VariantGroup, PropDoc for better demo organization

Added Interactive Components showcase section demonstrating ViewToggle with state management

Demonstrated responsive contexts: mobile (375px), tablet (768px), desktop (1024px)

Fixed timeline status style to handle 'building' state

Verification: cargo check passed (web-ui)

Commit: 38c9e939

All 7 acceptance criteria verified and satisfied

AC #1: Canonical isolation surface at /style-guide with Component Isolation Surface view

AC #2: Taxonomy with Primitives, Composites, Page Widgets, Design Tokens, Interactive Components

AC #3: Comprehensive typed fixtures (5 fixture types, 5 builder functions)

AC #4: State matrix pattern with StateMatrix and StateTile shells

AC #5: Responsive preview with 4 breakpoints (mobile/tablet/desktop/wide) + ResponsiveGrid

AC #6: Discoverable as 'Component Showcase' in navigation sidebar

AC #7: All checks passing (cargo fmt, cargo check verified)

Ready for final verification and review
<!-- SECTION:NOTES:END -->
