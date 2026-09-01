---
id: TASK-450.6
title: >-
  Share Rust compilation across agent worktrees with a compiler cache in the
  devshell
status: Backlog
assignee: []
created_date: '2026-08-31 22:39'
labels: []
dependencies: []
documentation:
  - >-
    backlog/docs/build/build-invalidation-graph/doc-23 -
    Build-Invalidation-Graph-and-CI-Feedback-Latency-Analysis.md
parent_task_id: TASK-450
priority: medium
type: enhancement
ordinal: 6000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

The agent workflow uses one dedicated Git worktree per task, and several task worktrees exist at the same time. Each worktree keeps a private Cargo `target/` directory, so each worktree compiles the same dependency crates independently.

The devshell in `shells/default` configures the Rust toolchain but sets neither `RUSTC_WRAPPER` nor `CARGO_TARGET_DIR`. `sccache` is not referenced anywhere in the repository.

This cost is paid on every `cargo` invocation an agent makes during iteration, which is the innermost and most frequent feedback loop in the whole workflow.

## Goal

Let compatible `rustc` outputs be reused across worktrees, so a newly created task worktree does not repeat compilation that another worktree already performed.

A shared compiler cache is preferred over a single shared `CARGO_TARGET_DIR`. A shared target directory causes concurrent worktrees to contend on the same Cargo lock and metadata, which serializes agents working in parallel. A compiler cache lets each worktree keep private Cargo metadata while sharing the expensive compilation.

## Constraints

The cache must not change build results. Cached and uncached builds must produce equivalent behavior.

The cache location must be a per-user cache directory that respects the usual environment overrides, and it must not be written inside the repository or any worktree.

Nix-sandboxed builds are unaffected by this task. This is a developer and agent inner-loop improvement only, and it must not alter the hermetic package or check builds.

Entering the devshell must still succeed if the cache directory cannot be created or the cache tool fails; a broken cache must degrade to a normal build rather than block development.

## Context

Read doc-23, `Build Invalidation Graph and CI Feedback Latency Analysis`, for the surrounding design and the constraints that apply across this initiative.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The development shell configures a Rust compiler cache that is shared across worktrees for the same user
- [ ] #2 The cache directory lives under a per-user cache path outside the repository and honors the standard cache-directory environment override
- [ ] #3 A recorded measurement shows a fresh worktree reusing compilation performed in another worktree, comparing a cold and a warm build of the same target
- [ ] #4 Hermetic Nix package and check builds are unaffected, evidenced by unchanged derivation hashes for a build that did not otherwise change
- [ ] #5 Entering the development shell still succeeds when the cache directory is unavailable, and the failure mode is documented
- [ ] #6 Developer-facing documentation explains the cache, where it stores data, and how to clear it
<!-- AC:END -->
