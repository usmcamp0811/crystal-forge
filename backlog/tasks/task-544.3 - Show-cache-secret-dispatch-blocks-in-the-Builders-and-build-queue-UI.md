---
id: TASK-544.3
title: Show cache-secret dispatch blocks in the Builders and build queue UI
status: To Do
assignee: []
created_date: '2026-11-24T23:27:00Z'
labels:
  - builders
  - ui
dependencies:
  - TASK-544.2
parent_task_id: TASK-544
priority: medium
type: enhancement
ordinal: 582000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem
TASK-544 slice C. Builders keep heartbeating, so the UI shows them 'ready' while every poll is blocked or refused.

## Goal
The Builders view and the build queue show when queued work is blocked by an undecryptable cache secret.

## Non-goals
Server dispatch (544.1), config-health (544.2), CVE worker (544.4). No new fault source; this slice only presents what 544.2 reports.

## Architectural constraints
UI composes presentation and interaction; the state decision lives outside view markup. Use the existing DTO/API patterns; no direct database access. Preserve loading, empty, error and authorization states.

## Verification plan
Nix farm builds. Task-owned preview database and live preview per docs/agents/live-preview.md; MR screenshot. Owning integration scenario exercises the state.

## Impact areas
web-ui builders and build-queue views, the DTO that carries the block state, cf-server read endpoint feeding it, integration-delivery, docs/knowledge.

## Risk
Low. Presentation only.

## Dependencies
TASK-544.2.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 When a cache-secret fault blocks dispatch, the Builders view and the build queue show a blocked state naming the destination and column, and do not show 'ready' alone.
- [ ] #2 With no fault the views are unchanged.
- [ ] #3 Owning integration scenario covers both states; MR includes a screenshot and live preview URL.
<!-- AC:END -->
