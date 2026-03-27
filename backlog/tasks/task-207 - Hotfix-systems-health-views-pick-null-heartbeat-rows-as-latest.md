---
id: TASK-207
title: Hotfix systems health views pick null heartbeat rows as latest
status: Review
assignee: []
created_date: '2026-03-21 20:25'
updated_date: '2026-03-21 20:39'
labels:
  - backend
  - database
  - hotfix
  - bug
  - high-priority
dependencies: []
references:
  - packages/default/migrations/0077_create_view_system_detail_with_hardware.sql
  - packages/default/migrations/0078_create_view_system_list.sql
  - packages/default/src/services/systems.rs
priority: high
ordinal: 1600
---

# Hotfix systems health views pick null heartbeat rows as latest

---

# Problem Statement

Systems that are actively heartbeating can still appear Offline in the systems list and system detail views.

Observed behavior shows `last_seen` updating to the current time while `health_status` remains `offline`. This misrepresents live systems and blocks operators from trusting fleet health indicators.

---

# Goal

Systems with recent recorded heartbeats appear Healthy/Warning/Critical according to heartbeat recency in both the systems list and system detail views.

---

# Non-Goals

- Changing heartbeat thresholds
- Refactoring the system health model beyond the hotfix
- Redesigning systems UI presentation
- Changing agent payload structure or agent networking

---

# Acceptance Criteria

- [ ] `view_system_list` selects the latest non-null heartbeat timestamp for each system when one exists
- [ ] `view_system_detail` selects the latest non-null heartbeat timestamp for each system when one exists
- [ ] A system with a recent `agent_heartbeats.timestamp` no longer reports `offline` in API responses solely because a newer `system_states` row lacks a heartbeat row
- [ ] Existing `last_seen` behavior continues to reflect the latest heartbeat or state timestamp
- [ ] Regression coverage proves a system with both heartbeat-backed and heartbeat-less state rows resolves to a non-offline health status when a recent heartbeat exists

---

# Architectural Constraints

- Keep the change minimal and focused on the health-view bug
- Follow existing API/view patterns; no UI-layer health recomputation
- Prefer a view/migration hotfix over broad service refactors
- No schema changes unless strictly required

---

# Verification Plan

Automated:
- `nix develop -c cargo test systems`
- `nix develop -c cargo test views`
- `nix develop -c cargo fmt -- --check`

Manual:
- Query the system list/detail API for a host with a recent heartbeat and a newer heartbeat-less state row
- Verify `health_status` matches heartbeat recency instead of `offline`
- Verify `last_seen` still reflects the newest heartbeat or state timestamp

---

# Impact Areas

API | Database

- System list view SQL
- System detail view SQL
- Systems API responses

---

# Risk Level

Low

The likely fix is a targeted ordering correction in existing SQL views plus regression coverage.

---

# Dependencies

None

---

# Follow-Up Tasks (if discovered during execution)

- Consider consolidating health-status computation into a single shared DB view/helper if duplication becomes error-prone

---

# Implementation Notes

LOCK: OpenCode on gray in /home/mcamp/code/crystal-forge/TASK-207-systems-health-view-hotfix

MR: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/179
