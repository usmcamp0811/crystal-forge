---
id: TASK-266
title: 'Fix: primary IP not showing in Systems list view'
status: Backlog
assignee: []
created_date: '2026-04-11 18:15'
labels:
  - bug
  - systems
  - api
  - ui
dependencies: []
references:
  - packages/default/src/api/models.rs
  - packages/default/src/services/systems.rs
  - packages/web-ui/src/api/models.rs
  - packages/web-ui/src/views/systems_list.rs
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

The Systems view table never shows a primary IP for any system — it always shows `-`.

## Root Cause (already traced)

Three-layer gap:

1. **Backend `SystemSummary` struct** (`packages/default/src/api/models.rs` ~line 397) has no `primary_ip` field — the field only exists on `SystemDetail`.
2. **`list_row_to_summary`** (`packages/default/src/services/systems.rs` ~line 283) therefore cannot map `row.primary_ip_address` even though `SystemListRow` does fetch it from the DB view.
3. **Frontend `SystemSummary`** (`packages/web-ui/src/api/models.rs` ~line 344) has `primary_ip: Option<String>` with `#[serde(default)]` so it always deserialises as `None`.

## Desired Outcome

- Add `primary_ip: Option<String>` to the backend `SystemSummary` API model.
- Map `row.primary_ip_address` → `primary_ip` in `list_row_to_summary`.
- IP column in the Systems table shows the agent-reported IP for systems that have checked in.

## Non-Goals

- No schema or DB view changes needed — `primary_ip_address` is already in `view_system_list` / `SystemListRow`.
- No UI layout changes beyond the field now having a value.
<!-- SECTION:DESCRIPTION:END -->
