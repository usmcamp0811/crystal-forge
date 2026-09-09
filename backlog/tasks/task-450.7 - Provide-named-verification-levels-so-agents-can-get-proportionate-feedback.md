---
id: TASK-450.7
title: Provide named verification levels so agents can get proportionate feedback
status: Backlog
assignee: []
created_date: '2026-08-31 22:40'
updated_date: '2026-08-31 22:41'
labels: []
dependencies:
  - TASK-450.4
documentation:
  - >-
    backlog/docs/build/build-invalidation-graph/doc-23 -
    Build-Invalidation-Graph-and-CI-Feedback-Latency-Analysis.md
  - docs/agents/verification.md
  - AGENTS.md
parent_task_id: TASK-450
priority: medium
type: enhancement
ordinal: 7000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

`docs/agents/verification.md` and `AGENTS.md` already tell agents to choose the smallest set of commands that proves the acceptance criteria. That policy exists only as prose, so an agent must assemble the right command for itself. In practice agents either under-verify or reach for a release-scale command such as `nix flake check` during iteration, which is the most expensive way to discover a typo.

## Goal

Express the existing verification policy as named, runnable entry points with predictable latency, so the cheap path is the obvious path.

Three levels are intended:

- a fast level for use during coding, targeting seconds to about one minute
- a component level for use after finishing a logical unit, targeting a few minutes on a warm store
- a full level for use once before moving a task to Review

## Constraints

This task adds entry points and documentation. It must not change what any existing check does, and it must not weaken any existing gate.

The full level must remain the authoritative pre-Review verification. Nothing here may give the impression that a fast level result is sufficient evidence for Review.

The levels must be honest about their limits. Each level must state what it does not prove, so an agent does not treat a fast pass as a guarantee.

The latency targets are goals for a warm store after the build-graph subtasks land. If a level cannot meet its target, record the measured latency rather than quietly redefining the level.

## Context

Read doc-23, `Build Invalidation Graph and CI Feedback Latency Analysis`, for the surrounding design and the constraints that apply across this initiative.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Three named verification entry points exist and are discoverable from the repository development environment
- [ ] #2 Each level documents what it proves, what it does not prove, and when an agent should run it
- [ ] #3 The measured latency of each level is recorded against its target on a warm store
- [ ] #4 AGENTS.md and docs/agents/verification.md reference the named levels instead of describing the policy only in prose
- [ ] #5 The documentation states explicitly that the release-scale full flake check is not an iterative debugging command
- [ ] #6 No existing check behavior or merge gate is changed by this task
<!-- AC:END -->
