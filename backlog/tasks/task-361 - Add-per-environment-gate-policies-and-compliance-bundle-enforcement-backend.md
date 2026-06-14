---
id: TASK-361
title: Add per-environment gate policies and compliance bundle enforcement backend
status: Backlog
assignee: []
created_date: '2026-06-14 19:10'
labels:
  - environments
  - policies
  - compliance
  - backend
  - design-parity-followup
milestone: 'm-19: Design Parity Existing Surfaces'
dependencies: []
priority: medium
ordinal: 304000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem
The Environments design reference shows per-environment policy enforcement: a multi-select of à-la-carte gate policies and a required compliance bundle selector. The backend currently models only "required_policy_ids" (agent baseline), not deploy-gate policies or compliance bundles. TASK-358 renders the gate-policy picker and compliance selector from clearly-commented placeholders.

## Desired Outcome
Add backend schema (new migration), queries, and API to associate gate policies and a compliance bundle with an environment, returning them in the environment payload. Wire the Environments Add/Edit modal (TASK-358) gate policy picker and compliance bundle selector to authoritative data. Coordinate with the Policies and Compliance views' data models.
<!-- SECTION:DESCRIPTION:END -->
