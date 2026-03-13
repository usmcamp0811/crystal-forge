---
id: TASK-188
title: Frontend Component Isolation Platform + Engineering Standards Documentation
status: Backlog
assignee: []
created_date: '2026-03-13 01:40'
labels:
  - frontend
  - ux
  - docs
  - architecture
dependencies: []
references:
  - packages/web-ui/src/views/style_guide.rs
  - packages/web-ui/src/components/
  - packages/web-ui/src/views/dashboard.rs
  - packages/web-ui/src/views/builds.rs
  - packages/web-ui/src/views/systems_list.rs
  - packages/web-ui/src/routes.rs
documentation:
  - docs/specs/01-frontend-views.md
  - docs/architecture.md
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

Crystal Forge has reusable and semi-reusable UI elements (cards, timelines, queue widgets, status rows, warning banners, table/list rows), but many are still easiest to inspect only within full page views. This slows visual iteration, increases regressions, and makes it harder to validate edge states (empty/loading/error/overflow) before integration.

## Goal

Establish a first-class component isolation workflow so UI components can be developed, reviewed, and regression-checked independently from page-level flows. Define and document a durable frontend standard so future contributors follow the same approach.

## Non-Goals

- This task does NOT redesign the full product visual language.
- This task does NOT migrate every component in one pass; it should define a practical minimum baseline and a repeatable process.
- This task does NOT replace page-level integration testing.
- This task does NOT require introducing Storybook if an in-repo route-based showcase is preferred.

## Scope

Create a structured "component isolation" surface (using the existing Style Guide route or a dedicated showcase route) where core components and complex view widgets can be rendered in isolation with state variants and realistic mock data.

This includes:

1. Isolation framework/pattern inside the existing web UI codebase
2. Initial onboarding set of high-value components/widgets migrated into isolated demos
3. State matrix for each showcased component (happy path + edge/failure states)
4. Documentation in repo describing best practices, conventions, and contribution workflow
5. A lightweight governance rule: new reusable components must include isolation demos and documented states

## Architectural Constraints

- Prefer extending existing `Style Guide` infrastructure first (`/style-guide`) unless a dedicated route is clearly superior.
- Keep isolation demos in-repo and deterministic (no external hosted tooling required).
- Use typed fixture builders for demo data; avoid ad-hoc inline JSON blobs where possible.
- Components should separate presentation from data orchestration so isolated rendering is straightforward.
- Avoid business logic in demo components; isolate UI primitives and composite widgets.
- Ensure mobile and desktop demo coverage for each showcased component.

## Best Practices To Codify In Docs

- Distinguish **UI primitives**, **composite components**, and **page containers**.
- Every reusable component must have documented supported states:
  - loading
  - empty
  - success/normal
  - error
  - long-content/overflow
  - permission-limited (if applicable)
- Components accept explicit props and avoid hidden global state dependencies.
- Use stable fixture factories under a shared test/demo fixtures location.
- Keep accessibility basics explicit (semantic headings, button labels, contrast expectations, keyboard focus visibility).
- Document when to create a new component vs extending an existing one.
- Document review checklist for visual consistency and responsive behavior.

## Impact Areas

- `packages/web-ui/src/views/style_guide.rs` (or new showcase route/view)
- `packages/web-ui/src/components/` (component extraction and prop cleanup)
- `packages/web-ui/src/routes.rs` (if new route is introduced)
- `packages/web-ui/src/*` for fixture/module organization
- `docs/` for frontend engineering standards and contributor workflow

## Risk Level

Medium — broad touch area in frontend structure and conventions, but primarily additive. Main risk is over-scoping migration; enforce a minimum baseline and iterative rollout.

## Verification Plan

- Tier 0:
  - `nix develop -c cargo fmt -- --check`
  - `nix develop -c cargo clippy -- -D warnings`
  - `nix develop -c cargo test` (targeted web-ui/frontend tests where available)
- Tier 1:
  - Run web UI and manually verify isolated demos for required state matrix and responsive behavior.
  - Validate at least one extracted complex widget from dashboard/build/systems views renders correctly in isolation.
- Tier 2:
  - `nix develop -c nix flake check` (required due to cross-cutting frontend architecture/doc standard change likely affecting multiple packages and CI expectations).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A documented in-repo component isolation surface exists (either expanded `/style-guide` or a dedicated showcase route) and is discoverable from navigation or contributor docs.
- [ ] #2 At least 8 high-value reusable/composite UI elements are rendered in isolation with realistic fixture data (including examples from dashboard/build queue/timeline/status-related UI).
- [ ] #3 Each showcased component includes a visible state matrix covering loading, empty, success, error, and long-content/overflow states.
- [ ] #4 At least 3 showcased components include explicit mobile and desktop variants (or responsive resize verification guidance) demonstrating layout integrity.
- [ ] #5 Complex page widgets currently coupled to page containers are refactored as needed so presentation components can render in isolation via props.
- [ ] #6 A shared fixture strategy is implemented and documented (e.g., fixture builders/helpers) to avoid duplicated ad-hoc demo data.
- [ ] #7 No business/domain mutation logic is embedded in isolated demo components; demos remain presentation-focused.
- [ ] #8 A new documentation page in `docs/` defines Crystal Forge frontend component engineering standards for isolation-driven development.
- [ ] #9 The documentation includes a contributor workflow for adding new components: extract -> fixture -> isolated demo -> state matrix -> integration usage.
- [ ] #10 The documentation includes a PR review checklist section for visual quality, responsiveness, accessibility basics, and edge-state coverage.
- [ ] #11 The team standard is codified: new reusable components must include isolation demos and required states before merge (or an explicitly documented exception).
- [ ] #12 Verification instructions are included in docs for running and reviewing the isolation surface locally in Nix dev environment.
- [ ] #13 Existing full-page views continue to function after extraction (no regression in dashboard/build/systems pages).
<!-- AC:END -->
