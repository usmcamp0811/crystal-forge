---
id: TASK-64
title: Refine flakes git history card density and visual styling
status: To Do
assignee: ["KimiK2.5"]
created_date: '2026-02-19 13:26'
labels:
  - ui
  - flakes
  - styling
dependencies: []
priority: medium
milestone: m-10
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Problem
Flakes view git history cards are too wide and visually flat, reducing scanability and causing unnecessary horizontal emphasis.

Goal
Make git history cards narrower and improve visual styling (background fills, spacing, and hierarchy) while preserving readability and responsiveness.

Non-Goals
- No backend/API changes.
- No changes to sync semantics or git data model.

Verification Plan
- Validate desktop card width and spacing in flakes view.
- Validate responsive behavior on tablet/mobile breakpoints.
- Run nix build .#checks.x86_64-linux.web-ui.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Git history cards are measurably narrower in desktop layout
- [ ] #2 Card styling uses intentional fill/background treatment and improved hierarchy
- [ ] #3 Cards remain readable at tablet/mobile widths
- [ ] #4 No regressions to existing flakes actions
- [ ] #5 nix build .#checks.x86_64-linux.web-ui passes
<!-- AC:END -->
