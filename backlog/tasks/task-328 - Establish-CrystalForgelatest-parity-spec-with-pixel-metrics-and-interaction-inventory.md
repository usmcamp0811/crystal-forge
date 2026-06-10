---
id: TASK-328
title: >-
  Establish CrystalForgelatest parity spec with pixel metrics and interaction
  inventory
status: In Progress
assignee:
  - gpt-5.4
created_date: '2026-05-31 15:56'
updated_date: '2026-06-10 19:37'
labels:
  - design-parity
  - ui-ux
  - planning
milestone: m-18
dependencies: []
references:
  - /home/mcamp/code/crystal-forge/CrystalForgelatest
  - /home/mcamp/code/crystal-forge/CrystalForgelatest/styles.css
  - /home/mcamp/code/crystal-forge/CrystalForgelatest/app.jsx
  - design/doc-8 - CrystalForgelatest-UI-Parity-Matrix-TASK-328.md
  - design/doc-9 - M16-Baseline-UI-Parity-Scorecard-Initial.md
priority: high
ordinal: 4000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Problem: We do not yet have a single executable parity spec mapping every CrystalForgelatest surface to current web-ui implementation, making exact parity subjective.

Goal: Produce and maintain the canonical parity matrix for all target views/components in `/home/mcamp/code/crystal-forge/CrystalForgelatest`, including visual tokens, spacing/typography rules, component states, interaction flows, and owner files.

## Non-goals
- Implementing UI changes.
- Changing API contracts in this task.

## Scope details
- Inventory all design-source pages/components.
- Define measurable pixel/value standards per surface.
- Map each design element to current web-ui ownership.
- Define screenshot and assertion requirements consumed by downstream parity tasks.

## Architectural constraints
- Planning/spec work only; do not modify product UI or server behavior in this task.
- Keep the matrix authoritative for downstream parity execution and align it with `design/doc-14 - Parity-execution-playbook-agent-proof.md`.
- Measurable criteria must use objective values/states rather than subjective wording.

## Impact areas
- `design/doc-8 - CrystalForgelatest-UI-Parity-Matrix-TASK-328.md`
- parity-related backlog tasks that consume the matrix
- web-ui screenshot/assertion planning

## Risk level
- Medium: an incomplete or ambiguous spec will misdirect downstream parity work.

## Verification plan
- Review the completed matrix against the CrystalForgelatest source tree for full coverage of primary surfaces.
- Confirm each row names owner files, screenshot targets, and mandatory assertions.
- Confirm loading/empty/error/populated states are specified where relevant.

## Notes
- This task remains the authoritative parity-spec contract for downstream UI parity work and should be completed before dependency-bound execution tasks such as `TASK-329`.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 A complete design parity matrix exists covering all primary views represented in CrystalForgelatest
- [x] #2 Each matrix row includes measurable criteria (pixel/value based) not subjective language
- [x] #3 Each matrix row maps to owning implementation files in packages/web-ui
- [x] #4 A screenshot target list for web-ui checks is defined for all in-scope views
- [x] #5 Interaction inventory includes filter/search/toggle/modal/table/card flows per relevant view
- [x] #6 The parity matrix defines mandatory web-ui assertions per view/state (not screenshot-only checks)
- [x] #7 The parity matrix requires screenshot coverage for all in-scope states including loading, empty, error, and populated states
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Audit the current parity matrix against `CrystalForgelatest/components/*` and `app.jsx` to identify missing surfaces, states, owner mappings, and assertion gaps.
2. Update `backlog/docs/design/doc-8 - CrystalForgelatest-UI-Parity-Matrix-TASK-328.md` so every in-scope surface has objective criteria, owner files, interaction inventory coverage, mandatory assertions, and screenshot targets for loading/empty/error/populated states plus view-specific variants.
3. Re-read the updated matrix against the design-source file list and the task acceptance criteria to verify full coverage and keep the matrix aligned with `doc-14`.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Updated `doc-8` into a full surface matrix covering shell, all primary CrystalForgelatest views, explicit owner files, mandatory interactions, and screenshot requirements for dark/light plus loading/empty/error/populated states.

Verified the resulting backlog/doc state is clean after the document tool update (`git status --porcelain` showed no remaining changes in both `dev` and the task worktree).
<!-- SECTION:NOTES:END -->
