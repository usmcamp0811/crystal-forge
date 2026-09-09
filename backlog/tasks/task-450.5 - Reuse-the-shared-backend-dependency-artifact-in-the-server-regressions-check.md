---
id: TASK-450.5
title: Reuse the shared backend dependency artifact in the server-regressions check
status: Backlog
assignee: []
created_date: '2026-08-31 22:39'
updated_date: '2026-08-31 22:41'
labels: []
dependencies:
  - TASK-450.4
documentation:
  - >-
    backlog/docs/build/build-invalidation-graph/doc-23 -
    Build-Invalidation-Graph-and-CI-Feedback-Latency-Analysis.md
parent_task_id: TASK-450
priority: medium
type: enhancement
ordinal: 5000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

`checks/server-regressions` builds its own Rust derivation independently of the server package. It therefore compiles substantially the same third-party dependency graph a second time, and it recompiles that graph whenever application source changes.

`server-regressions` is one of the four entries in the `.gitlab-ci.yml` heavy check matrix, so this duplicated compilation is paid on every merge request push.

## Goal

Make the regression check consume the shared backend dependency artifact introduced by the dependency-split subtask, so it compiles only local crates.

## Dependency

This task requires the shared backend dependency artifact to exist. Do not start it before that artifact is merged; the pattern it must follow is defined there.

## Constraints

The check must continue to run exactly the regression tests it runs today. Reducing compilation cost must not silently reduce test scope. If the set of executed tests changes for any reason, that change must be deliberate and stated.

## Context

Read doc-23, `Build Invalidation Graph and CI Feedback Latency Analysis`, for the surrounding design and the constraints that apply across this initiative.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The server-regressions check consumes the shared backend dependency artifact rather than compiling third-party crates independently
- [ ] #2 The set of regression tests executed by the check is unchanged, evidenced by comparing the executed test list before and after
- [ ] #3 A source-only backend change rebuilds only local crates for this check, demonstrated by a recorded before-and-after timing on an otherwise warm store
- [ ] #4 The server-regressions check passes
<!-- AC:END -->
