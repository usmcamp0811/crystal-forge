---
id: TASK-260
title: Improve "Link a flake" warning UX in Systems view
status: Backlog
assignee: []
created_date: '2026-04-11 00:00'
labels:
  - ux
  - systems-view
  - flakes
  - web-ui
milestone: m-12
dependencies: []
references:
  - packages/web-ui/src/views
  - packages/default/src/handlers/api/systems.rs
priority: medium
ordinal: 9200
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Problem:
In Systems view, the warning "1 system is not linked to a flake and won't be included in evaluations" links to the Flakes view, but does not clearly show which flake/system actions are needed. Users land on Flakes without enough context and it is not obvious how to resolve the warning.

Desired outcome:
Make the warning actionable by showing affected system(s) and clear next steps to resolve. The workflow should make it obvious how to link a flake to the impacted system(s) so the warning can be cleared.
<!-- SECTION:DESCRIPTION:END -->
