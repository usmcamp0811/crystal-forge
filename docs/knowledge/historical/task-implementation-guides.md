---
type: Historical Reference
title: "Task implementation guides"
description: "Catalogs the TASK-288 Evaluations view checklist (doc-6) and the TASK-297 Flakes view implementation guide (doc-7), two task-specific UI rebuild guides kept for their reasoning."
tags:
  - crystal-forge
  - web-ui
  - backlog
  - implementation-guide
  - evaluations
  - flakes
implementation_status: historical
status: deprecated
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-03T22:54:52-05:00
sources:
  - id: s1
    resource: "Crystal Forge repository file backlog/docs/doc-6%20-%20TASK-288-Complete-Implementation-Checklist.md at commit 3b23d36f"
    title: "TASK-288: Complete Implementation Checklist"
  - id: s2
    resource: "Crystal Forge repository file backlog/docs/doc-7%20-%20TASK-297-Complete-Implementation-Guide-Rebuild-Flakes-View.md at commit 3b23d36f"
    title: "TASK-297 Complete Implementation Guide - Rebuild Flakes View"
---
# Task implementation guides

This catalog describes two long task-specific Backlog documents that guided single UI rebuild tasks. Backlog.md manages them by ID, so they stay at their paths under `backlog/docs/`. The retained files are the authoritative text. Each guide targets a JSX design mockup that is not in this repository by the path the guide gives.

> **Status:** historical. Each guide belongs to one task and records its plan, not the current code.

## Catalog

### doc-6, TASK-288 Complete Implementation Checklist

File: [doc-6 - TASK-288-Complete-Implementation-Checklist.md](../../../backlog/docs/) (file `doc-6 - TASK-288-Complete-Implementation-Checklist.md`) (249 lines, type specification, created 2026-05-04).

A checklist for TASK-288, "Rebuild Evaluations View to Match JSX Mockup Design Exactly". It opens with 15 major structural differences (single-column versus two-column split, header buttons, stat strip, tab bar, active queue as a table, empty state, history filter bar and table, log modal, icons, CSS class system, data formatting, interactions). Sections A to O give checkbox items per area: page structure, header, stat strip, tab bar, active queue table, empty state, history filter bar and table, log modal, icons, CSS class migration, data models, behavior, typography, and pagination. It ends with a verification checklist and risk areas.

Status evidence: the task file `backlog/archive/tasks/task-288 - Rebuild-Evaluations-View-to-Match-JSX-Mockup-Design-Exactly.md` records status Done, last updated 2026-05-26. The checklist items are unchecked in the retained document and were not compared with `packages/web-ui/src/views/evaluations.rs`.

### doc-7, TASK-297 Complete Implementation Guide

File: [doc-7 - TASK-297-Complete-Implementation-Guide-Rebuild-Flakes-View.md](../../../backlog/docs/) (file `doc-7 - TASK-297-Complete-Implementation-Guide-Rebuild-Flakes-View.md`) (2,650 lines, type guide, created 2026-05-13).

A phased implementation guide for TASK-297, "Rebuild Flakes View to Match JSX Design Mockup Exactly". It states current and target state (a dual Table and Cards view, a side tray commit explorer, a time-bucketed commit timeline, a file diff modal, and Eval, Build, and Rollout pipeline status), an estimated effort of 12 to 16 hours, and a planned file structure with new components `flake_tray.rs`, `pipeline_status.rs`, and `diff_modal.rs`. It then walks eight phases (main view structure, side tray, commit timeline, commit detail panel, pipeline components, diff modal, table and cards views, integration and testing). It closes with a CSS class reference, data models, API requirements, a testing checklist, common issues, a completion checklist, and notes for the implementer.

Status evidence: `packages/web-ui/src/components/flake/` now holds `flake_timeline.rs`, `sync_chip.rs`, and `sync_error_banner.rs`. None of the planned `flake_tray.rs`, `pipeline_status.rs`, or `diff_modal.rs` files exists, and a separate `packages/web-ui/src/components/diff/diff_viewer.rs` exists. The subtask `TASK-297.1`, "Remove legacy FlakesListView implementation after FlakesListViewNew parity", was To Do at its last update (2026-06-28), and `packages/web-ui/src/views/flakes_list.rs` still exists. The guide is a draft plan, and the implementation diverged from its planned file structure. The guide names an absolute local design path and is kept as written.

## Related concepts

- [UI parity program planning documents](ui-parity-program-documents.md)
- [Crystal Forge UI/UX Design System](../ui/design-system-overview.md)
