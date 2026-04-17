---
id: TASK-142
title: Decouple Add System hostname from flake system config selection
status: Backlog
assignee: []
created_date: '2026-02-28 22:17'
labels:
  - ui
  - systems
  - flakes
dependencies:
  - TASK-140
references:
  - packages/web-ui/src/views/systems_list.rs
  - packages/default/src/queries/flakes.rs
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Problem: Add System currently infers/flattens relationship between agent hostname and flake system config name, which is brittle because flakes can change outputs across commits and may not contain a matching system. Desired outcome: In Add System flow, selecting a flake should default the system config name from hostname heuristics, but always allow operator override in UI and persist that explicit config name separately from host identity with lax validation across commits.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Add System UI shows flake selector plus editable system-config-name field
- [ ] #2 System-config-name field defaults from current hostname mapping heuristic when available
- [ ] #3 Operator can override system-config-name before save
- [ ] #4 Backend/API stores and returns explicit system-config-name independent of hostname
- [ ] #5 Validation remains lax: missing config in selected flake/commit does not hard-block create
<!-- AC:END -->

## Definition of Done
<!-- DOD:BEGIN -->
- [ ] #1 Document defaulting/override behavior in systems workflow docs
<!-- DOD:END -->
