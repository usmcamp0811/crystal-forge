---
id: TASK-328
title: >-
  Establish CrystalForgelatest parity spec with pixel metrics and interaction
  inventory
status: To Do
assignee: []
created_date: '2026-05-31 15:56'
updated_date: '2026-06-10 17:45'
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
- [ ] #1 A complete design parity matrix exists covering all primary views represented in CrystalForgelatest
- [ ] #2 Each matrix row includes measurable criteria (pixel/value based) not subjective language
- [ ] #3 Each matrix row maps to owning implementation files in packages/web-ui
- [ ] #4 A screenshot target list for web-ui checks is defined for all in-scope views
- [ ] #5 Interaction inventory includes filter/search/toggle/modal/table/card flows per relevant view
- [ ] #6 The parity matrix defines mandatory web-ui assertions per view/state (not screenshot-only checks)
- [ ] #7 The parity matrix requires screenshot coverage for all in-scope states including loading, empty, error, and populated states
<!-- AC:END -->
