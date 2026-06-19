---
id: TASK-340.3
title: >-
  Persist policy category, severity, rationale, evidence, and
  rollout/approval/time-window rules
status: Backlog
assignee: []
created_date: '2026-06-19 15:19'
labels:
  - policies
  - backend
  - design-parity
  - schema
dependencies: []
references:
  - TASK-340.1
  - >-
    /home/mcamp/code/crystal-forge/CrystalForgelatest/components/PoliciesView.jsx
  - packages/web-ui/src/components/policy/policy_editor_modal.rs
parent_task_id: TASK-340
priority: medium
ordinal: 308000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem
The design-faithful policy editor modal (TASK-340.1) renders several fields and rule/evidence types that the current deployment-policy backend does not persist. They are currently shown in the UI but flagged as UI-only/not-persisted.

UI-only fields/types with no backend support today:
- Policy `category` (deployment / pipeline / rollout / security)
- Policy `severity` (high/medium/low — CAT I/II/III)
- Policy `rationale`
- Evidence-for-ATO records (command, log, file, unit_state, eval_attr, attestation)
- Rule kinds without enforcement/persistence: `time_window`, `approval_required`, `rollout_percent` (and pipeline gate markers `eval_passed`, `build_succeeded` as first-class rules)

## Desired Outcome
Provide backend model/API/schema (and SQLx metadata) to persist and round-trip these fields/types so the policy modal can drop the UI-only/not-persisted markers, and wire enforcement where applicable. Likely needs migration(s), DTO updates in both server and web-ui API models, and handler/validator changes.

## Notes
- Current backend persists only: name, description, policy_type, config (JSON), enabled.
- Coordinate with the compliance roadmap (evidence overlaps with ATO/compliance surfaces).
<!-- SECTION:DESCRIPTION:END -->
