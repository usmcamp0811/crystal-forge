---
id: TASK-450
title: Reduce build and CI feedback latency by correcting the Nix invalidation graph
status: Backlog
assignee: []
created_date: '2026-08-31 22:38'
labels: []
dependencies: []
references:
  - 'https://crane.dev/API.html'
  - 'https://nix-gitlab-ci.projects.tf/caching/'
documentation:
  - >-
    backlog/docs/build/build-invalidation-graph/doc-23 -
    Build-Invalidation-Graph-and-CI-Feedback-Latency-Analysis.md
  - docs/agents/verification.md
priority: high
type: enhancement
ordinal: 461000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Umbrella task. Do not implement this task directly; implement its subtasks.

## Problem

A small source change invalidates large Rust builds, and several Nix checks compile substantially the same Rust dependency graph more than one time. Humans and automated agents both wait far longer than the size of their change justifies. Agents in particular have no cheap, trustworthy way to get a pass or fail signal, so they either under-verify or start release-scale checks during iteration.

The cause is the shape of the build graph, not a shortage of compute. Adding build machines would make unnecessary work finish sooner without removing the unnecessary work.

## Goal

Make the cost of feedback proportional to the size of the change.

A backend source change must not recompile the whole Cargo dependency tree. A web UI change must not rebuild the backend server used by unrelated checks. A builder change must not invalidate the server derivation. A documentation change must not start VM-based integration checks.

## Outcome

- Agents have three named verification levels with predictable latency: fast during coding, component-scoped after a logical unit, and full once before Review.
- Merge request pipelines run only the checks that the change can affect, and superseded pipelines stop.
- Reporting jobs that cannot fail a merge no longer sit in the per-push feedback path.
- The authoritative production guarantee is preserved: one check still proves the production server binary, the embedded production WASM, and a real browser together before merge.

## Context for implementers

Read the design document `Build Invalidation Graph and CI Feedback Latency Analysis` (doc-23) before starting any subtask. It records the verified current state with exact file locations, the target architecture, the required implementation order, and the constraints that must hold.

Subtask order matters. Phases 1 to 4 remove false invalidation edges. The caching and CI phases only pay off after those edges are gone, because before the dependency split a one-character source change always produces a cache miss for the expensive compilation.

## Non-goals

- Adding remote builders or additional CI runner capacity. That is deliberately deferred until the graph is corrected.
- Removing any existing public flake output. Aggregate packages remain as compatibility outputs.
- Rewriting every Rust package in the repository onto a new build framework at once.
- Reducing test coverage, deleting checks, or weakening the pre-merge quality gate.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 All subtasks of this parent are Done or explicitly withdrawn with a recorded reason
- [ ] #2 A recorded before-and-after measurement shows the rebuild cost of a backend-source-only change, a web-UI-only change, and a builder-only change
- [ ] #3 No internal NixOS module, check, or lib helper depends on an aggregate Crystal Forge package
- [ ] #4 Aggregate flake outputs still exist and still resolve for external consumers
- [ ] #5 One authoritative pre-merge check still exercises the production server binary with the embedded production WASM in a real browser
<!-- AC:END -->
