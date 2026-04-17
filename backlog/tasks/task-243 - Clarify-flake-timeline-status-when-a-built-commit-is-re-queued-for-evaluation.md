---
id: TASK-243
title: Clarify flake timeline status when a built commit is re-queued for evaluation
status: Backlog
assignee: []
created_date: '2026-04-04 01:28'
labels:
  - ui
  - timeline
  - evaluation
  - build-status
dependencies: []
references:
  - packages/default/src/queries/flakes.rs
  - packages/default/src/queries/commits.rs
  - packages/default/src/handlers/api/commits.rs
  - packages/web-ui/src/components/flake/flake_timeline.rs
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

The flake timeline can currently show a commit with contradictory-looking state such as:
- `eval: queued`
- `build: complete`

This happens because the two statuses are derived independently:

- `build_status` is aggregated from existing `build_jobs` for the commit
- `evaluation_status` comes from commit-level evaluation state and can be reset back to `pending` / queued by manual re-evaluation

So a commit that already built successfully can later be re-queued for evaluation, producing a confusing UI that looks like builds happened before evals.

## Goal

Make flake timeline status consistent and understandable when a previously built commit is re-evaluated.

## Desired Outcome

A re-queued evaluation of an already-built commit should not look like an impossible pipeline ordering bug. The UI and underlying query/model should clearly represent whether the commit:
- previously built successfully
- is currently queued/running for re-evaluation
- should suppress old build state, preserve it, or explicitly show both in a non-confusing way

## Key Decision Needed

When a commit with existing successful build history is manually re-evaluated, what should the timeline show?

**Recommended:** preserve prior build history, but explicitly present the commit as `re-evaluation queued` / `re-evaluation running` so the UI does not imply builds happened before the first eval.

Alternative options:
1. Latest eval state wins: hide/suppress prior build-complete while re-evaluation is queued/running
2. Keep current dual-state model but change rendering text so it is clearly a re-evaluation, not initial evaluation

## Scope

Investigate and likely update:
- `packages/default/src/queries/flakes.rs` (there are two different timeline query paths today)
- `packages/default/src/queries/commits.rs`
- `packages/default/src/handlers/api/commits.rs` (`re_evaluate_commit` flow)
- `packages/web-ui/src/components/flake/flake_timeline.rs`

## Notes

There are currently two timeline query paths with different evaluation-status derivation logic in `queries/flakes.rs`, which likely contributes to inconsistent presentation across screens.

This is probably not a one-line fix because it requires a product/semantic decision, not just a bug patch.
<!-- SECTION:DESCRIPTION:END -->
