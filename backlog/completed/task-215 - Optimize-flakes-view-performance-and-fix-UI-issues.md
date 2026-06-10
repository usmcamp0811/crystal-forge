---
id: TASK-215
title: Optimize flakes view performance and fix UI issues
status: Done
assignee: []
created_date: ''
updated_date: '2026-06-10 02:54'
labels: []
milestone: 'm-10: UI Views - Flakes'
dependencies: []
priority: high
ordinal: 1000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Optimize flakes view performance and resolve UI issues captured in the original task scope. This task was merged through the dedicated task branch and no additional execution remains on backlog.
<!-- SECTION:DESCRIPTION:END -->

## Problem

The flakes view has multiple critical issues making it painful to use:

**Performance:**
- Takes ~60 seconds to load when viewing a flake
- Backend reads commit data from disk on every page load
- No database caching of frequently accessed commit metadata

**UI Accuracy:**
- Evaluation status shows misleading "eval: failed" for policy failures
  - Example: "❌ Evaluation Error: 2 systems failed strict deployment policies"
  - This is NOT an evaluation error - evaluation succeeded, some systems failed policy
  - Should indicate "partial success" or "not all systems passed"

**UI Theming:**
- System status chips not correctly themed
- Inconsistent visual hierarchy and color usage

**Timezone:**
- All timestamps shown in UTC
- No browser timezone detection
- No user-configurable timezone preference

## Goal

Make the flakes view load instantly (<2 seconds) and fix all UI accuracy/theming issues.

## Desired Outcome

**Performance:**
- Flakes view loads in <2 seconds (down from ~60 seconds)
- Commit metadata cached in database for recent commits
- Automatic garbage collection of old cached data

**UI Accuracy:**
- Evaluation status correctly distinguishes:
  - ✅ Complete (all systems passed policies)
  - ⚠️ Partial (some systems failed policies, some passed)
  - ❌ Failed (evaluation error - Nix syntax error, etc.)
- Chip labels clearly communicate state without ambiguity

**UI Theming:**
- All status chips use consistent, correctly themed colors
- Visual hierarchy matches semantic meaning (error=red, warning=yellow, success=green, info=blue)

**Timezone:**
- Timestamps display in browser's local timezone by default
- Timezone preference stored per-user (future: allow manual override)

## Non-Goals

- Changing evaluation logic (already fixed in TASK-213)
- Real-time streaming of commit data (keep polling)
- Caching ALL commits forever (only recent N commits)
- Complex timezone UI (just use browser default for now)

## Acceptance Criteria
<!-- AC:BEGIN -->
**Critical (must have):**
- [x] #1 Task was merged into dev and no additional review action remains in backlog.

**Important (should have):**

**Nice to have:**
<!-- AC:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Merged into dev; stale Review state cleared during backlog cleanup.
<!-- SECTION:FINAL_SUMMARY:END -->
