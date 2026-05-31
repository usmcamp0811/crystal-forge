---
id: TASK-328
title: >-
  Establish CrystalForgelatest parity spec with pixel metrics and interaction
  inventory
status: To Do
assignee: []
created_date: '2026-05-31 15:56'
updated_date: '2026-05-31 16:07'
labels:
  - design-parity
  - ui-ux
  - planning
milestone: m-16
dependencies: []
references:
  - /home/mcamp/code/crystal-forge/CrystalForgelatest
  - /home/mcamp/code/crystal-forge/CrystalForgelatest/styles.css
  - /home/mcamp/code/crystal-forge/CrystalForgelatest/app.jsx
priority: high
ordinal: 1600
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Problem: We do not yet have a single executable parity spec mapping every CrystalForgelatest surface to current web-ui implementation, making exact parity subjective.

Goal: Produce a canonical parity matrix for all target views/components in /home/mcamp/code/crystal-forge/CrystalForgelatest, including visual tokens, spacing/typography rules, component states, and interaction flows.

Non-goals: Implementing UI changes; changing API contracts in this task.

Scope details:
- Inventory all design-source pages/components (Systems, Flakes, Builds, Evals, CVEs, Caches, Compliance, Admin, shared shell/sidebar/topbar/tweaks).
- Define measurable pixel standards per surface: spacing, radius, typography sizes/weights, color tokens, borders, shadows, breakpoints, table row heights, chip dimensions, modal geometry, and empty/loading/error states.
- Create a parity checklist that maps each design element to web-ui file ownership.
- Define acceptance screenshot set required for web-ui check.

Verification plan:
- Generate a markdown parity matrix doc under backlog docs.
- Confirm every in-scope view has at least one reference screenshot target and one interaction scenario.

Impact areas: packages/web-ui (all view layers), API payload consumers.
Risk: Medium (spec quality drives all downstream work).
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
