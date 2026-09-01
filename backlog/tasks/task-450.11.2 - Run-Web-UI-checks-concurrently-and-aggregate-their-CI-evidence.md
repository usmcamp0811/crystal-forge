---
id: TASK-450.11.2
title: Run Web UI checks concurrently and aggregate their CI evidence
status: In Progress
assignee:
  - opencode-gpt-5.6-sol
created_date: '2026-09-01 03:27'
updated_date: '2026-09-01 18:07'
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
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/325'
modified_files:
  - .gitlab-ci.yml
  - ci/web-ui-producer.sh
  - ci/web-ui-aggregate.js
  - ci/web-ui-ci.test.js
  - docs/web-ui-check.md
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
1. Remove `web-ui` from the generic flake matrix and add a required five-entry Web UI producer matrix plus one advisory design-parity producer, all with aligned merge-request/main rules and safe cancellation.
2. Add a focused producer script that records the gate exit status, resolves and copies the gate's already-realized `.evidence` store output without a second VM build, publishes it under `web-ui-evidence/<check>/`, records machine-readable producer status, and preserves blocking failure semantics while identifying evidence infrastructure failures.
3. Add one `when: always`, non-blocking aggregator that downloads all producer artifacts with `needs`, reports every expected producer and detailed evidence, owns MR uploads/commenting, and always retains a report artifact.
4. Add fixture-driven Node tests for producer and aggregation behavior, update the Web UI CI runbook, and validate scripts, tests, Nix evaluation, and GitLab YAML syntax with repository/Nix tooling.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
The user selected this task for one focused Web UI optimization MR with TASK-438, TASK-354, TASK-450.11.1, and TASK-450.11.3.

LOCK: opencode-gpt-5.6-sol in /home/mcamp/code/crystal-forge/TASK-450-web-ui-parallel-checks on branch TASK-450.11-web-ui-parallel-checks, based on TASK-450-p0-build-graph at 437efd55.

Implemented the CI partition on top of the existing uncommitted Nix work. The generic matrix retains integration, oidc-auth, and server-regressions. A required five-check Web UI matrix and advisory design-parity producer publish collision-free evidence. The producer performs one gate build, resolves the already-realized `.evidence` store path without a second build, records status metadata, and preserves gate failure status. One `when: always` advisory aggregator uses `needs`, reports all expected producers and detailed evidence, owns pipeline-specific MR uploads/comments, and preserves its report on API failure.

Verification passed: `nix develop -c node --test ci/web-ui-ci.test.js checks/web-ui/tests/browser-verdict.test.js checks/web-ui/tests/check-groups.test.js` (14/14); `nix develop -c node checks/web-ui/tests/validate-check-groups.js` (100 steps valid); `nix run nixpkgs#shellcheck -- ci/web-ui-producer.sh`; `nix run nixpkgs#yq-go -- eval 'true' .gitlab-ci.yml`; `nix eval --json 'path:.#checks.x86_64-linux' --apply builtins.attrNames` (all six Web UI attributes present); and `git diff --check`. VM checks were not executed because this CI-only task verifies orchestration around the existing partition and running all six would require the target CI/KVM runner capacity. No commit, push, or MR was created as requested.
<!-- SECTION:NOTES:END -->
