---
id: TASK-363
title: >-
  Populate parent_derivation_id during eval to enable per-system package counts
  in dep graph
status: Backlog
assignee: []
created_date: '2026-06-18 15:36'
labels: []
dependencies: []
priority: low
ordinal: 307000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The dep graph tab in the eval detail drawer currently shows one row per NixOS system with a simple built/to-build/failed status. The user asked for the bars to reflect the number of individual packages within each system that need to be built vs pulled from cache vs already built — matching the build view's stacked derivation progress bars.

The data model has the scaffolding (`parent_derivation_id` column, `package` derivation type, `derivation_dependencies` table) but it is not populated during evaluation in production. `package`-type derivations have no `commit_id` and no `parent_derivation_id` link to their `nixos` parent.

To enable this feature:
1. During eval, when packages are discovered for a system, set `parent_derivation_id = nixos_derivation.id` and `commit_id` on each `package`-type derivation row
2. Verify `parent_derivation_id` column exists on all deployed instances (add migration if needed — do NOT edit existing migrations)
3. Update `fetch_eval_dependency_breakdown` query to JOIN `package` derivations via `parent_derivation_id`, grouping by nixos system name, counting ready/pending/failed package derivations
4. Update the dep graph UI to show `X/total built · Y to build` subtitle and stacked bars per system

Reference: the JOIN query was prototyped in TASK-345.1 commit history but reverted because the column/data was not available on deployed servers.
<!-- SECTION:DESCRIPTION:END -->
