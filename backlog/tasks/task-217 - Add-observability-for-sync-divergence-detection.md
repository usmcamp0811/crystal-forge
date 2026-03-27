---
id: TASK-217
title: Add observability for sync divergence detection
status: In Progress
assignee: []
created_date: '2026-03-26 17:00'
updated_date: '2026-03-26 17:00'
labels:
  - hotfix
  - logging
  - flakes
dependencies: []
priority: high
---

## Description

## Problem

Production troubleshooting needs deterministic visibility into why per-flake sync returns `0 updated` instead of surfacing a rewrite conflict.

## Goal

Add explicit structured logs in the incremental sync path that print:
- DB since hash
- remote branch head hash
- divergence decision
- final action (return zero updates vs emit history rewrite conflict)

## Acceptance Criteria

- Logs clearly show divergence decision inputs and outcome for each sync call.
- No functional behavior changes outside observability.

## Verification Plan

- Targeted unit test still passes for divergence helper.
- Package compiles in SQLX offline mode.

## Lock

LOCK: opencode-gpt-5.3-codex on reckless in /home/mcamp/code/crystal-forge/TASK-217-sync-divergence-logging
