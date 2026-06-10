---
id: TASK-331
title: >-
  Close parity gaps across existing web-ui views (Flakes, Builds, Evals, CVEs,
  Caches)
status: Backlog
assignee: []
created_date: '2026-05-31 15:56'
updated_date: '2026-06-10 02:57'
labels:
  - design-parity
  - multi-view
  - web-ui
milestone: m-19
dependencies:
  - TASK-328
  - TASK-329
references:
  - /home/mcamp/code/crystal-forge/CrystalForgelatest
modified_files:
  - packages/web-ui/src/views/flakes*.rs
  - packages/web-ui/src/views/builds.rs
  - packages/web-ui/src/views/evaluations.rs
  - packages/web-ui/src/views/cves.rs
  - packages/web-ui/src/views/caches.rs
priority: high
ordinal: 1630
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Problem: Several existing views are partially migrated or visually inconsistent with CrystalForgelatest and each other.

Goal: Bring the existing non-systems primary views that already exist in Crystal Forge to exact parity with reference design standards and interaction patterns.

Non-goals: Net-new missing surfaces such as Compliance, Admin, and Profile; those are tracked separately under m-20.

Replan note: this task has been narrowed to reduce overlap and support faster vertical-slice execution.

Scope details:
- Validate and align Flakes, Builds, Evaluations, CVEs, and Caches for typography, spacing, cards/tables, status chips, timeline/list styling, controls, and modal surfaces.
- Ensure consistent shell integration and section rhythm.
- Resolve per-view visual regressions and density mismatches.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Each listed view has validated pixel-parity screenshots against design references
- [ ] #2 Shared interaction patterns (filters/tabs/modals/table density) behave consistently across views
- [ ] #3 No listed view relies on mock/fallback data in production path for primary content
- [ ] #4 All identified parity defects are tracked and closed or split into explicit follow-up tasks
- [ ] #5 web-ui check includes assertion-based verification for each listed view's critical interactions
- [ ] #6 web-ui check captures screenshots for all listed views across loading, empty, error, and populated states
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Reset to Backlog and narrowed to existing implemented surfaces only.
<!-- SECTION:NOTES:END -->
