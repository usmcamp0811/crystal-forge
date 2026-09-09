---
id: TASK-450.9
title: Move coverage and complexity reporting out of the per-push feedback path
status: Backlog
assignee: []
created_date: '2026-08-31 22:40'
labels: []
dependencies: []
documentation:
  - >-
    backlog/docs/build/build-invalidation-graph/doc-23 -
    Build-Invalidation-Graph-and-CI-Feedback-Latency-Analysis.md
parent_task_id: TASK-450
priority: medium
type: enhancement
ordinal: 9000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

Two expensive jobs occupy correctness-gate positions while returning no result that can block a merge.

`coverage-check` runs `nix run .#coverage.coverage-report`, which invokes `cargo tarpaulin --all-features --workspace` (`packages/coverage/default.nix:38-47`). The wrapper catches Tarpaulin failure and the script ends with `exit 0` at `packages/coverage/default.nix:262`.

`complexity-check` runs `nix run .#code-metrics.complexity-report`, which invokes `cargo clippy --all-targets --all-features` with `|| true` appended (`packages/code-metrics/default.nix:67`).

Both are gated by `only: [merge_requests, main]`, so both compile a large Rust graph on every push. Instrumented and `--all-features` builds do not share compilation with the normal build, so this work is close to pure overhead in the iteration loop.

## Goal

Keep both reports, and stop paying for them on every push.

These are reporting operations. They belong on a schedule, on an explicit opt-in, or at the point where a merge request becomes ready for review, rather than in the loop an agent iterates against.

## Decision required from the implementer

Choose and record a trigger policy. Reasonable options include a scheduled run, an explicit merge request label, running once when a merge request enters Review, or running only when backend Rust source changed. State which policy was chosen and why in the task notes.

## Constraints

Do not delete the reports, the merge request comment behavior, or the published artifacts. This task moves when the jobs run.

Do not convert a non-blocking report into a blocking gate as a side effect. If the project later wants clippy to block a merge, that is a separate decision with its own task, not an accident of rescheduling.

Whatever trigger is chosen must be discoverable. A reviewer must be able to tell why a report is present or absent on a given merge request.

## Context

Read doc-23, `Build Invalidation Graph and CI Feedback Latency Analysis`, for the surrounding design and the constraints that apply across this initiative.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Coverage reporting no longer runs on every merge request push
- [ ] #2 Complexity reporting no longer runs on every merge request push
- [ ] #3 The chosen trigger policy for each report is implemented and its rationale is recorded
- [ ] #4 Both reports are still produced, still published as artifacts, and still posted to the merge request when they run
- [ ] #5 Neither report becomes a blocking merge gate as a result of this change
- [ ] #6 A reviewer can determine from the pipeline why a report ran or did not run for a given merge request
<!-- AC:END -->
