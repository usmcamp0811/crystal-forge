---
id: TASK-152
title: Increase user dropdown width in TopLayout
status: Backlog
assignee: []
created_date: '2026-03-01 18:39'
labels:
  - ui
  - frontend
  - styling
dependencies: []
references:
  - >-
    class: "absolute top-full mt-2 w-100 {theme::surface::CARD_BG} border
    {theme::surface::CARD_BORDER} rounded-lg shadow-xl z-50"
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem
In TopLayout, the user dropdown container uses `w-100`, but changing this class does not affect the rendered width as expected. The dropdown remains too narrow for its content.

## Desired Outcome
Update the TopLayout user dropdown styling so the menu is visibly wider (slightly wider than current behavior), using the project’s existing CSS/theme conventions and preserving responsive behavior.
<!-- SECTION:DESCRIPTION:END -->
