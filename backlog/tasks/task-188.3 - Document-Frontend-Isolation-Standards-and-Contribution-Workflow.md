---
id: TASK-188.3
title: Document Frontend Isolation Standards and Contribution Workflow
status: Done
assignee: []
created_date: '2026-03-13 01:52'
updated_date: '2026-03-17 03:08'
labels:
  - frontend
  - docs
  - governance
dependencies:
  - TASK-188.1
  - TASK-188.2
references:
  - docs/
  - packages/web-ui/src/views/style_guide.rs
  - packages/web-ui/src/components/
documentation:
  - docs/architecture.md
  - docs/specs/01-frontend-views.md
parent_task_id: TASK-188
priority: high
ordinal: 4000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

Without explicit engineering standards, component isolation practices become inconsistent over time. Contributors may skip isolated states, rely on page-only validation, or introduce components with hidden dependencies.

## Goal

Create durable repository documentation that codifies Crystal Forge frontend best practices for isolation-driven development and defines the required contributor workflow for new reusable components.

## Non-Goals

- This task does NOT implement large new UI features.
- This task does NOT require replacing existing architecture docs beyond targeted updates/additions.
- This task does NOT create policy outside repository docs (team process tools are out of scope).

## Scope

1. Add a dedicated documentation page for frontend component engineering standards.
2. Define taxonomy and layering rules (primitives, composites, page containers).
3. Define required component state coverage and responsiveness expectations.
4. Define fixture strategy and demo organization conventions.
5. Define contribution workflow: extract -> fixture -> isolated demo -> state matrix -> integrate.
6. Add PR review checklist section specific to frontend component quality.
7. Link docs from relevant existing docs and/or developer entry points.

## Architectural Constraints

- Documentation must reflect real codebase paths and workflows established in TASK-188.1 and TASK-188.2.
- Standards must be actionable and testable (avoid vague style guidance).
- Keep guidance framework-appropriate to Dioxus + existing CF frontend conventions.
- Ensure instructions include Nix-based local verification commands.

## Required Doc Topics

- Why isolation-driven development matters in CF
- Component classification and boundaries
- Mandatory state matrix for reusable components
- Fixture builder conventions and directory structure
- Responsive verification expectations
- Accessibility baseline checks
- "Definition of merge-readiness" for new reusable components
- Exception process (when a component can merge without full isolation coverage)

## Impact Areas

- New docs page under `docs/` (frontend standards)
- Cross-links from relevant docs (`docs/architecture.md` and/or frontend specs)
- Optional references in contributor onboarding docs if present

## Risk Level

Low-Medium — docs-first change, but risk of drift if not aligned with implementation.

## Verification Plan

- Tier 0:
  - Validate markdown links and code path references are correct.
  - Confirm commands in docs are executable in Nix dev environment.
- Tier 1:
  - Walk through documented workflow against at least one extracted component from TASK-188.2 to ensure instructions are practical.
- Tier 2:
  - Not required unless doc build tooling enforces broader checks.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A new documentation page exists in `docs/` defining Crystal Forge frontend component isolation standards.
- [ ] #2 The doc defines component taxonomy (primitives/composites/page containers) and boundary rules with concrete CF examples.
- [ ] #3 The doc mandates required state coverage for reusable components: loading, empty, success, error, long-content/overflow, and permission-limited where applicable.
- [ ] #4 The doc defines fixture conventions (shared typed builders, location, naming) and bans ad-hoc duplicated fixture blobs.
- [ ] #5 The doc defines responsive verification expectations and minimum viewport checks.
- [ ] #6 The doc includes an accessibility baseline checklist for isolated component reviews.
- [ ] #7 The doc includes a step-by-step contributor workflow: extract -> fixture -> isolated demo -> state matrix -> integration usage.
- [ ] #8 The doc includes a frontend PR review checklist covering visual consistency, edge states, responsive behavior, and accessibility basics.
- [ ] #9 The docs codify that new reusable components require isolation demos before merge, with an explicit documented exception path.
- [ ] #10 The docs include Nix-based local verification instructions for running and reviewing the isolation surface.
- [ ] #11 Relevant existing docs are updated with links to the new standards page so contributors can discover it easily.
- [ ] #12 All documented paths, component names, and commands are verified against the repository and are not stale placeholders.
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
LOCK: Claude (OpenCode) on gray in ~/code/crystal-forge/TASK-188.3-document-frontend-standards

MR created: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/168

Documentation: docs/frontend-component-standards.md (767 lines)

Covers: taxonomy, state coverage, fixtures, responsive, a11y, workflow, review checklist, merge requirements, exceptions

Cross-links: architecture.md, web-ui-coding-standards.md, specs/01-frontend-views.md, CONTRIBUTING.md

All paths/commands verified against repository

Ready for review

MR #168 merged to dev on 2026-03-17

Task complete - all 12 acceptance criteria satisfied.
<!-- SECTION:NOTES:END -->
