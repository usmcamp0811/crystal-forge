---
id: TASK-450.11.3
title: Validate and tune the parallel Web UI gate below 20 minutes
status: To Do
assignee: []
created_date: '2026-09-01 03:28'
updated_date: '2026-09-01 03:30'
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

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
The user selected this task for one focused Web UI optimization MR with TASK-438, TASK-354, TASK-450.11.1, and TASK-450.11.2.
<!-- SECTION:NOTES:END -->
