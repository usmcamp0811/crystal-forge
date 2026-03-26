---
id: TASK-216
title: Ensure history rewrite modal triggers on sync divergence
status: In Progress
assignee: []
created_date: '2026-03-26 15:55'
updated_date: '2026-03-26 15:55'
labels:
  - bug
  - flakes
  - hotfix
dependencies: []
priority: high
---

## Description

## Problem

In production, clicking "Sync from Source" can return "0 updated" even when the remote branch has moved and the DB tip is stale after a history rewrite. This prevents the `history_rewrite_detected` conflict from being surfaced, so the rewrite acceptance modal never appears.

## Goal

Always surface a rewrite conflict when DB tip diverges from remote HEAD in per-flake sync, including cases where the git range command does not emit `Invalid revision range` but still yields no commits.

## Non-Goals

- Redesigning the full flakes table warning UX (tracked separately)
- Changing audit data model beyond existing event + log behavior

## Acceptance Criteria

- Per-flake sync returns `409 history_rewrite_detected` when DB latest commit hash differs from remote HEAD and no new commits were ingested.
- Flakes UI modal appears for this conflict path when user clicks "Sync from Source" on selected flake.
- Existing non-conflict sync behavior remains unchanged.

## Verification Plan

- Targeted backend tests for divergence detection helper logic.
- `SQLX_OFFLINE=true cargo check -p crystal-forge` in `packages/default`.
- `cargo check` in `packages/web-ui`.

## Risk

Medium: incorrect divergence detection could over-trigger conflicts. Keep detection narrowly scoped to `updated == 0 && latest_db_hash != remote_head_hash`.

## Lock

LOCK: opencode-gpt-5.3-codex on reckless in /home/mcamp/code/crystal-forge/TASK-216-rewrite-warning-indicator
