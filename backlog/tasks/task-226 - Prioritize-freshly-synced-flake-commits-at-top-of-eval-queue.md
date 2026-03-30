---
id: TASK-226
title: Prioritize freshly synced flake commits at top of eval queue
status: Backlog
assignee: []
created_date: '2026-03-30 01:56'
labels:
  - queueing
  - flakes
  - evaluation
  - scheduling
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

When a flake is refreshed and new commits are discovered, those commits are not consistently evaluated first. Operators expect newly refreshed commits to be processed immediately, but current queue ordering can delay them behind older work.

## Desired Outcome

Whenever `Sync from Source` (or equivalent refresh/sync path) inserts new commits for a flake, the newest discovered commit(s) should be promoted to the top of the evaluation queue so evaluation starts promptly.

## Notes

- Scope should cover both API-triggered sync and any shared queue-insertion path used by refresh/sync flows.
- Preserve queue fairness safeguards where possible, but prioritize explicit operator-triggered fresh commits.
<!-- SECTION:DESCRIPTION:END -->
