---
id: TASK-330
title: >-
  Complete Systems view parity including cards-table modes and modal flows with
  real API data
status: To Do
assignee: []
created_date: '2026-05-31 15:56'
updated_date: '2026-06-11 21:41'
labels:
  - design-parity
  - systems
  - api-integration
milestone: m-19
dependencies:
  - TASK-328
  - TASK-329
references:
  - /home/mcamp/code/crystal-forge/CrystalForgelatest/app.jsx
modified_files:
  - packages/web-ui/src/views/systems_list.rs
  - packages/web-ui/src/components/system
  - packages/web-ui/src/systems/adapter.rs
  - packages/web-ui/src/api/models.rs
priority: high
ordinal: 1620
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Problem: Systems page is partially aligned but still drifts from design behavior/visuals and includes fallback/local-state patterns that can diverge from backend truth.

Goal: Achieve exact Systems view parity with CrystalForgelatest for both cards and table modes, with all displayed values sourced from real backend APIs.

Non-goals: New domain features not present in design examples.

Replan note: reset to Backlog as an m-19 vertical slice to be resumed after m-18 foundation tasks are complete enough to prevent rework.

Scope details:
- Align page header, stat strip, filter bar, cards/table geometry, chip styles, and selected-state behaviors.
- Match detail panel and modal visual/interaction behavior (deploy/edit/remove/update key).
- Remove/contain fallback/mock rendering in production path; ensure API-driven values for counts, statuses, and metadata.
- Ensure loading/empty/error states are styled per design and tied to real API outcomes.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Systems view card and table layouts are pixel-aligned with reference for core states
- [ ] #2 Filters/search/view toggles reproduce design behavior and result counts
- [ ] #3 All stat-strip and row/card values are sourced from backend APIs in production path
- [ ] #4 All systems modals/panels match reference spacing/typography/interactions
- [ ] #5 web-ui checks include screenshot + behavior assertions for systems parity scenarios
- [ ] #6 web-ui screenshot set covers loading, empty, error, and populated states for Systems surfaces
<!-- AC:END -->
