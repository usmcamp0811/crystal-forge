---
id: TASK-219
title: Guard progressive timeline fetches with generation token
status: Done
assignee: []
created_date: '2026-03-26 23:20'
updated_date: '2026-03-31 01:57'
labels:
  - hotfix
  - ui
  - reliability
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

Progressive flake timeline loading can race when a newer refresh starts while older batch requests are still in-flight, allowing stale responses to overwrite newer state.

## Goal

Add request-generation guarding so only the latest timeline load sequence can update UI state.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria

- Older timeline batch responses are ignored once a newer generation starts.
- Manual refresh and initial load preserve latest-state timeline rendering.
- No regression in progressive loading behavior.

## Verification Plan

- `cargo check` for web-ui
- Manual sanity check: trigger refresh while loading and confirm no stale overwrite

## Lock

LOCK: opencode-gpt-5.3-codex on reckless in /home/mcamp/code/crystal-forge/TASK-219-harden-progressive-timeline-race

## Review

MR: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/190
