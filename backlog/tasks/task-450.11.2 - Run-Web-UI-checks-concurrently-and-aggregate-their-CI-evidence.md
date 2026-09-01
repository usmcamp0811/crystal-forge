---
id: TASK-450.11.2
title: Run Web UI checks concurrently and aggregate their CI evidence
status: In Progress
assignee:
  - opencode-gpt-5.6-sol
created_date: '2026-09-01 03:27'
updated_date: '2026-09-01 03:46'
labels:
  - web-ui
  - testing
  - nix
  - playwright
  - gitlab-ci
  - ci-performance
dependencies:
  - TASK-450.11.1
references:
  - TASK-430
  - TASK-450.8
  - .gitlab-ci.yml
parent_task_id: TASK-450.11
priority: high
type: enhancement
ordinal: 2000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

Independent Web UI Nix checks do not reduce merge-request wall-clock time unless GitLab schedules them concurrently and reviewers can consume their outputs as one coherent result. The current CI job assumes one `web-ui` result directory and one screenshot source, so parallel jobs could overwrite artifacts or publish incomplete evidence.

## Goal

Run the complete Web UI check set concurrently across available GitLab runners. Treat every blocking check as one logical merge gate and combine their collision-free artifacts into one reviewer-facing report.

The pipeline must remain correct when fewer runners are available: jobs may queue or execute sequentially, but no required check may be skipped. Superseded safe jobs should remain compatible with TASK-450.8 cancellation behavior.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 GitLab schedules the independently runnable Web UI checks as separate jobs that can execute concurrently on available runners
- [ ] #2 Every blocking Web UI job is required for the logical merge gate and a failed or missing blocking job prevents merge success
- [ ] #3 Limited runner availability changes queueing or execution order but never skips a required check
- [ ] #4 Each job publishes artifacts under a stable collision-free path that identifies the responsible check and browser workflow group
- [ ] #5 One aggregation job combines statuses, failed step details, screenshots, visual reports, export evidence, and links from all Web UI jobs into a reviewer-facing result
- [ ] #6 Artifact aggregation runs after all required producers and clearly reports missing or failed producer output instead of presenting a complete success report
- [ ] #7 Shared Nix derivations are substituted or reused across jobs when available and the CI configuration does not intentionally rebuild identical inputs per shard
- [ ] #8 Safe long-running Web UI jobs support superseded-pipeline cancellation consistently with TASK-450.8
- [ ] #9 The CI and Web UI check documentation identifies which jobs are blocking, which are advisory, and how to locate each job's artifacts
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Keep non-UI checks in their existing matrix and add a dedicated blocking Web UI matrix whose entries match the new stable flake attributes.
2. For each producer, build the blocking check, recover its successful evidence derivation regardless of verdict, and publish under a check-specific collision-free directory before returning the blocking status.
3. Run design parity as a separate advisory producer. Mark deterministic side-effect-free Web UI producers interruptible.
4. Replace the single-producer screenshot assumption with one `when: always` aggregator that waits for all expected producers, identifies failed or missing output, combines reports and artifacts, and publishes one pipeline-specific reviewer comment.
5. Validate the expanded GitLab configuration, blocking/advisory semantics, artifact paths, failure handling, and behavior when runners serialize jobs.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
The user selected this task for one focused Web UI optimization MR with TASK-438, TASK-354, TASK-450.11.1, and TASK-450.11.3.

LOCK: opencode-gpt-5.6-sol in /home/mcamp/code/crystal-forge/TASK-450-web-ui-parallel-checks on branch TASK-450.11-web-ui-parallel-checks, based on TASK-450-p0-build-graph at 437efd55.
<!-- SECTION:NOTES:END -->
