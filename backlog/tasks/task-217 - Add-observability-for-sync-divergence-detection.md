---
id: TASK-217
title: Add observability for sync divergence detection
status: Review
assignee: []
created_date: '2026-03-26 17:00'
updated_date: '2026-04-02 00:04'
labels:
  - hotfix
  - logging
  - flakes
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

Production troubleshooting needs deterministic visibility into why per-flake sync returns `0 updated` instead of surfacing a rewrite conflict.

## Goal

Add explicit structured logs in the incremental sync path that print:
- DB since hash
- remote branch head hash
- divergence decision
- final action (return zero updates vs emit history rewrite conflict)
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria

- Logs clearly show divergence decision inputs and outcome for each sync call.
- No functional behavior changes outside observability.

## Verification Plan

- Targeted unit test still passes for divergence helper.
- Package compiles in SQLX offline mode.

## Lock

LOCK: opencode-gpt-5.3-codex on reckless in /home/mcamp/code/crystal-forge/TASK-217-sync-divergence-logging

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Backlog maintenance sync: task branch head `b8afb163` is contained in `dev` (merged). Transitioning task to Review for completion finalization.
<!-- SECTION:NOTES:END -->
