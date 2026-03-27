---
id: TASK-218
title: Progressive flake timeline loading for faster first render
status: In Progress
assignee: []
created_date: '2026-03-26 22:55'
updated_date: '2026-03-26 22:55'
labels:
  - hotfix
  - ui
  - performance
dependencies: []
priority: high
---

## Description

## Problem

Flakes view loads all flake timelines in one request (`/api/v1/flakes/timelines`) and blocks initial display when timeline response is slow.

## Goal

Load only a small initial subset for immediate render, then progressively hydrate remaining flake timelines in background.

## Acceptance Criteria

- Initial Flakes view shows quickly with first N flake timelines.
- Remaining timelines populate progressively without blocking first paint.
- Existing per-flake timeline rendering behavior remains intact.

## Verification Plan

- `cargo check` for web-ui
- Manual verification that Flakes page paints faster and timelines continue filling after first render

## Lock

LOCK: opencode-gpt-5.3-codex on reckless in /home/mcamp/code/crystal-forge/TASK-218-progressive-flake-timelines
