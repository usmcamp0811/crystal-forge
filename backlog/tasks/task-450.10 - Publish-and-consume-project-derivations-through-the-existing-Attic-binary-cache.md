---
id: TASK-450.10
title: >-
  Publish and consume project derivations through the existing Attic binary
  cache
status: Backlog
assignee: []
created_date: '2026-08-31 22:41'
updated_date: '2026-08-31 22:41'
labels: []
dependencies:
  - TASK-450.4
references:
  - 'https://docs.cachix.org/what-is-a-binary-cache'
  - 'https://nix-gitlab-ci.projects.tf/caching/'
documentation:
  - >-
    backlog/docs/build/build-invalidation-graph/doc-23 -
    Build-Invalidation-Graph-and-CI-Feedback-Latency-Analysis.md
parent_task_id: TASK-450
priority: medium
type: enhancement
ordinal: 10000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

CI runners, developer machines, and agent worktrees each rebuild the same project derivations independently. An Attic instance already exists, but the repository does not push project outputs to it or configure consumers to substitute from it.

## Goal

Let CI runners, developer machines, and agent worktrees substitute an already-built project derivation instead of rebuilding it.

## Dependency

This task requires the backend dependency artifact from the dependency-split subtask.

The ordering is not cosmetic. Before that split, any source change alters the server derivation hash, so the expensive compilation always misses the cache and the cache provides close to no benefit for iteration. After the split, the dependency derivation hash is stable across source-only changes, so the cache substitutes the expensive part.

## Constraints

An existing Attic instance is available; standing up new cache infrastructure is out of scope.

Push credentials are secrets. They must not appear in job logs, in the repository, in task notes, or in merge request descriptions. Only trusted pipeline contexts may push. Untrusted contexts, including merge requests from forks, must be able to read without being able to write.

Substitution requires a trusted public key. Cache configuration must not weaken signature verification or add an unverified substituter.

Cache availability must not become a build dependency. If the cache is unreachable or returns an error, builds must fall back to local building rather than fail.

Do not push transient or sensitive build outputs. Decide deliberately which outputs are published.

## Context

Read doc-23, `Build Invalidation Graph and CI Feedback Latency Analysis`, for the surrounding design and the constraints that apply across this initiative.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 CI publishes the selected project derivations to the existing Attic cache from trusted pipeline contexts only
- [ ] #2 CI runners and the development environment substitute those derivations from the cache instead of rebuilding them, demonstrated by a recorded cache hit on a machine that did not build the derivation
- [ ] #3 Push credentials never appear in job logs, repository files, task notes, or merge request descriptions
- [ ] #4 Signature verification remains enabled and the cache is trusted through its public key
- [ ] #5 A build succeeds when the cache is unreachable, demonstrated by an intentional unavailability test
- [ ] #6 The set of published outputs is deliberate and documented, and excludes outputs that should not be shared
- [ ] #7 Setup instructions exist for connecting a developer or agent machine to the cache
<!-- AC:END -->
