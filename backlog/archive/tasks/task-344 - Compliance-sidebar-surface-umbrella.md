---
id: TASK-344
title: Compliance sidebar surface umbrella
status: Backlog
assignee: []
created_date: '2026-06-10 03:35'
labels:
  - design-parity
  - compliance
  - umbrella
milestone: 'm-20: Design Parity Missing Surfaces'
dependencies: []
references:
  - TASK-320
  - TASK-319
  - TASK-334
  - >-
    /home/mcamp/code/crystal-forge/CrystalForgelatest/components/ComplianceView.jsx
  - design/doc-12 - Compliance-implementation-roadmap.md
  - design/doc-13 - Sidebar-surface-execution-map.md
documentation:
  - design/doc-10 - CrystalForgelatest-parity-execution-plan.md
  - design/doc-12 - Compliance-implementation-roadmap.md
  - design/doc-13 - Sidebar-surface-execution-map.md
modified_files:
  - packages/web-ui/src/views/compliance.rs
  - checks/web-ui/tests/integration-test.js
priority: high
ordinal: 1780
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Problem: Compliance work spans backend domain/evaluator tasks, backend-backed UX tasks, and final design-parity work, which makes it hard to understand the user-facing Compliance surface from a single task.

Desired Outcome: A single umbrella task exists for the Compliance sidebar surface so backend foundation, UX, and final parity tasks can all roll up into one user-facing record.

Scope: planning/coordination only. Direct implementation belongs in linked compliance backend/UI tasks.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A single umbrella record exists for the Compliance sidebar surface
- [ ] #2 Backend foundation, backend-backed UX, and final parity tasks are clearly linked from this umbrella
- [ ] #3 The Compliance surface is only considered complete when linked domain, UX, and parity tasks are complete
<!-- AC:END -->
