---
id: TASK-450.11.3
title: Validate and tune the parallel Web UI gate below 20 minutes
status: In Progress
assignee:
  - opencode-gpt-5.6-sol
created_date: '2026-09-01 03:28'
updated_date: '2026-09-01 17:34'
labels:
  - web-ui
  - testing
  - nix
  - playwright
  - gitlab-ci
  - ci-performance
dependencies:
  - TASK-450.11.2
references:
  - TASK-354
  - TASK-450.10
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/pipelines/2807718751'
parent_task_id: TASK-450.11
priority: high
type: enhancement
ordinal: 3000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

A parallel check topology can still miss the latency goal if one group is imbalanced, VM startup dominates, shared derivations miss the cache, or artifact publication remains on the critical path. A single successful run is not sufficient evidence because runner availability and cache state vary.

## Goal

Measure the complete parallel Web UI merge gate under representative GitLab conditions and tune responsibility boundaries until the blocking verdict consistently completes in less than 20 minutes without weakening coverage or reliability.

Measurements must distinguish execution time from runner queue time and must identify cache state. The final record must make future regressions attributable to a phase or check rather than only reporting one total duration.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Phase timing distinguishes Nix build or substitution, VM startup and fixture setup, each required browser group, export validation, design-parity processing, artifact transfer, and aggregation
- [ ] #2 At least three representative merge-request pipelines complete every blocking Web UI job in less than 20 minutes of critical-path execution time
- [ ] #3 The recorded results include each job duration, blocking critical path, runner queue time, cache state, median duration, and maximum duration
- [ ] #4 The measured pipelines preserve all required semantic workflows, production embedded-WASM smoke coverage, OSCAL validation, SARIF validation, screenshot evidence, and design-parity evidence
- [ ] #5 No measured success depends on suppressed failures, missing artifacts, unbounded retries, or unsupported timeout reductions
- [ ] #6 A deliberately failing required workflow demonstrates that the parallel logical gate fails and identifies the responsible check without requiring unrelated successful groups to be rerun locally
- [ ] #7 Documentation records the final check grouping, expected timing envelope, measurement method, exact local Nix commands, and how maintainers detect a latency regression
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Add phase timestamps for Nix realization, VM startup and fixtures, each browser group, export validation, design parity, artifact transfer, and aggregation.
2. Run the complete logical gate in representative merge-request pipelines and record job execution, queue, cache state, median, maximum, and critical path.
3. Rebalance only explicit step ownership or evidence placement when one check dominates. Do not remove required workflows or shorten waits below reliable observable conditions.
4. Demonstrate one deliberate blocking failure and one advisory design-parity failure with correct pipeline outcomes and retained evidence.
5. Record at least three sub-20-minute blocking critical-path runs and update the Web UI check documentation with the final timing envelope and regression-detection procedure.

Before opening the MR, extend producer metadata with pipeline/job timestamps, queue duration, Nix realization cache classification, and realization/artifact timings. Extend aggregation output with each producer duration, queue time, cache state, blocking critical path, per-pipeline median, and maximum. Use the pinned repository Nix environment for the aggregation job. Representative three-run medians and maxima remain a post-push evidence step.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
The user selected this task for one focused Web UI optimization MR with TASK-438, TASK-354, TASK-450.11.1, and TASK-450.11.2.

LOCK: opencode-gpt-5.6-sol in /home/mcamp/code/crystal-forge/TASK-450-web-ui-parallel-checks on branch TASK-450.11-web-ui-parallel-checks, based on TASK-450-p0-build-graph at 437efd55.

Pre-MR timing instrumentation is complete. Each producer records GitLab Jobs API `queued_duration` with a 5-second connection timeout and 15-second total timeout, Nix realization cache classification, job duration, gate realization, evidence lookup, and artifact copy. The aggregate report records per-producer timing and cache state and withholds median, maximum, and critical-path values unless all five blocking producers have valid durations. The runbook documents the 20-minute envelope and phase-based regression diagnosis.

Local final exports evidence completed in 35.288 seconds total (19.131 seconds VM fixture setup and 15.946 seconds exports). Fleet, pipeline, governance, compatibility, and design evidence timings remain recorded from the earlier successful local runs. Acceptance criteria requiring three representative MR pipelines and recorded cross-run median/maximum remain pending after push.
<!-- SECTION:NOTES:END -->
