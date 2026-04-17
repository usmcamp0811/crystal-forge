---
id: TASK-151
title: Remove temporary light-theme compatibility bridge and complete token migration
status: Backlog
assignee: []
created_date: '2026-03-01 17:37'
labels:
  - web-ui
  - theming
  - cleanup
  - tech-debt
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem
`packages/web-ui/assets/app.css` currently includes a temporary light-theme compatibility bridge that overrides broad legacy utility classes with `!important`. This keeps the UI readable but increases global style coupling and makes future behavior harder to reason about.

## Desired Outcome
Eliminate the compatibility bridge by migrating remaining legacy hardcoded utility usage to semantic token-backed classes, including status-tone primitives, and verify light/dark parity without global overrides.

## Notes
Discovered during TASK-141 review follow-up. This is intentionally out of current task scope and should be scheduled as dedicated cleanup work.
<!-- SECTION:DESCRIPTION:END -->
