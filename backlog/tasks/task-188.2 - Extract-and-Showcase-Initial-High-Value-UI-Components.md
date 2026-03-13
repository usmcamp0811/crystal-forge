---
id: TASK-188.2
title: Extract and Showcase Initial High-Value UI Components
status: To Do
assignee: []
created_date: '2026-03-13 01:52'
updated_date: '2026-03-13 12:15'
labels:
  - frontend
  - ux
dependencies:
  - TASK-188.1
references:
  - packages/web-ui/src/views/dashboard.rs
  - packages/web-ui/src/views/builds.rs
  - packages/web-ui/src/views/systems_list.rs
  - packages/web-ui/src/components/
documentation:
  - docs/specs/01-frontend-views.md
parent_task_id: TASK-188
priority: high
ordinal: 2000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

Even with a showcase foundation, Crystal Forge gains little unless key real-world widgets are extracted and represented with complete state coverage. Current high-impact components remain difficult to validate in isolation.

## Goal

Extract and render an initial baseline set of high-value reusable/composite components in the isolation surface, including complex dashboard/build/systems widgets, with full state matrices.

## Non-Goals

- This task does NOT migrate every frontend component in the repository.
- This task does NOT create new business behavior.
- This task does NOT finalize organization-wide governance docs (handled by another child task).

## Scope

1. Select and extract at least 8 high-value components/composites from active views.
2. Ensure extracted presentation components are prop-driven and isolation-friendly.
3. Add isolation demos for each selected component with required states.
4. Include at least 3 components with explicit responsive verification (mobile + desktop behavior).
5. Preserve existing page behavior where components were extracted.

## Suggested Candidate Areas

- Dashboard cards/widgets (e.g., build queue cards, timeline/status summaries)
- Build queue row/card variants
- Timeline visualization component(s)
- System status/policy badges or row summary blocks
- Warning/alert display units used across views

## Architectural Constraints

- Use shared fixture strategy established in TASK-188.1.
- Do not couple demos to live API calls.
- Keep business/data orchestration in view/container layers; extracted components remain presentational.
- Maintain existing theme tokens and style conventions.

## Impact Areas

- `packages/web-ui/src/views/dashboard.rs`
- `packages/web-ui/src/views/builds.rs`
- `packages/web-ui/src/views/systems_list.rs`
- `packages/web-ui/src/components/`
- isolation surface view/modules from TASK-188.1

## Risk Level

Medium — extraction can introduce regressions if boundaries are unclear; mitigated by keeping behavioral logic in existing containers.

## Verification Plan

- Tier 0:
  - `nix develop -c cargo fmt -- --check`
  - `nix develop -c cargo clippy -- -D warnings`
  - `nix develop -c cargo test`
- Tier 1:
  - Run web UI and validate all extracted components in isolation with state matrix coverage.
  - Validate responsive behavior for selected components.
  - Smoke-check dashboard/build/systems pages for regressions.
- Tier 2:
  - `nix develop -c nix flake check`
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 At least 8 high-value reusable/composite components are available in the isolation surface with realistic fixture data.
- [ ] #2 At least one extracted component originates from each of these areas: dashboard, builds, and systems.
- [ ] #3 Every showcased extracted component includes states for loading, empty, success, error, and long-content/overflow.
- [ ] #4 At least 3 showcased components demonstrate explicit responsive behavior for mobile and desktop layouts.
- [ ] #5 Extracted components are prop-driven and render without direct API calls or global mutation dependencies.
- [ ] #6 Existing dashboard/build/systems full-page views continue to function after extraction with no user-visible regressions.
- [ ] #7 Any newly created reusable component includes an isolation demo entry at the time it is introduced in this task.
- [ ] #8 Component extraction stays within scope of presentation/composition and does not change deployment/build/system business logic.
<!-- AC:END -->
