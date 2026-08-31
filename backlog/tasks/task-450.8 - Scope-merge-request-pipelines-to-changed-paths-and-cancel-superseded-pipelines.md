---
id: TASK-450.8
title: Scope merge request pipelines to changed paths and cancel superseded pipelines
status: Backlog
assignee: []
created_date: '2026-08-31 22:40'
labels: []
dependencies: []
references:
  - 'https://nix-gitlab-ci.projects.tf/caching/'
documentation:
  - >-
    backlog/docs/build/build-invalidation-graph/doc-23 -
    Build-Invalidation-Graph-and-CI-Feedback-Latency-Analysis.md
parent_task_id: TASK-450
priority: medium
type: enhancement
ordinal: 8000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

`.gitlab-ci.yml:182-188` runs a parallel matrix of the `integration`, `oidc-auth`, `server-regressions`, and `web-ui` checks. `.gitlab-ci.yml:215-217` gates that job with `only: [merge_requests, main]` and nothing else. There is no `rules:changes` clause and no `interruptible: true` anywhere in the file.

Two consequences follow.

First, every push starts the complete heavy matrix regardless of what changed. A documentation-only or backlog-only commit starts VM-based checks that boot PostgreSQL, Keycloak, Grafana, a Git server, and Chromium.

Second, a superseded pipeline keeps running. Agents commonly push, notice a problem, and push again minutes later. Runner capacity is then spent finishing checks for a commit nobody will merge, which delays feedback for the commit that matters.

## Goal

Run the checks a change can plausibly affect, and stop spending capacity on commits that have been replaced.

A ready merge request must still receive the complete gate before merge. Path scoping shortens the iteration loop; it must not become a way to merge without the checks that protect the affected behavior.

## Constraints

Scoping must be conservative. Changes to cross-cutting inputs such as the flake definition, the lock file, shared NixOS modules, shared library helpers, migrations, and the backend package definition must fan out broadly. When it is unclear whether a path affects a check, run the check.

Cancellation must only apply to jobs that are safe to interrupt. A job that performs an external side effect, such as publishing a tag or posting a report, must be reviewed before it is marked interruptible.

Path rules are a maintenance hazard: they are correct on the day they are written and silently wrong later. Record the intended mapping from path to check in a form a future maintainer can review, and make the fan-out default explicit.

## Context

Read doc-23, `Build Invalidation Graph and CI Feedback Latency Analysis`, for the surrounding design and the constraints that apply across this initiative.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Each heavy check runs only when the merge request changes a path that can affect it
- [ ] #2 A documentation-only or backlog-only change starts no VM-based check
- [ ] #3 Changes to cross-cutting inputs including the flake definition, the lock file, shared modules, shared lib helpers, migrations, and the backend package definition fan out to the full set of heavy checks
- [ ] #4 A merge request that is ready to merge still receives the complete heavy check set before it can be merged
- [ ] #5 Long-running jobs are marked interruptible, and any job excluded from cancellation is excluded deliberately with a recorded reason
- [ ] #6 Pushing a new commit to a merge request cancels the superseded pipeline, demonstrated on a real merge request
- [ ] #7 The mapping from changed path to selected check is documented where a future maintainer will find it
- [ ] #8 The fan-out default when a path is unmatched is explicit and conservative
<!-- AC:END -->
