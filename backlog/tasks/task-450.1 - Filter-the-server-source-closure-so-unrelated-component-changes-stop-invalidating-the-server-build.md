---
id: TASK-450.1
title: >-
  Filter the server source closure so unrelated component changes stop
  invalidating the server build
status: To Do
assignee: []
created_date: '2026-08-31 22:38'
labels: []
dependencies: []
documentation:
  - >-
    backlog/docs/build/build-invalidation-graph/doc-23 -
    Build-Invalidation-Graph-and-CI-Feedback-Latency-Analysis.md
parent_task_id: TASK-450
priority: high
type: enhancement
ordinal: 1000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

`packages/default/default.nix:65` sets `serverSrc = src;`, so the server derivation takes the entire backend package tree as its Nix input. The agent, builder, and keygen derivations already restrict their input with the `mkWorkspaceSrc` helper defined at `packages/default/default.nix:22`.

Because the server is unfiltered, a change under `crates/cf-builder` or `crates/cf-agent` changes the `cf-server-drv` derivation hash and forces a server rebuild, even though the `cf-server` manifest depends locally only on `cf-config` and `cf-protocol`. Every check that boots a server is invalidated with it.

## Goal

Restrict the server derivation input to workspace metadata plus the transitive local-crate closure that `cf-server` actually needs, using the same pattern the other three components already use.

This is the lowest-risk item in the parent task and should land first.

## Required care

`serverSrcHash` (`packages/default/default.nix:85`) is derived from `serverSrc` and is exported as `SRC_HASH` during the server build. Narrowing `serverSrc` changes the value and the meaning of that hash. Determine what reads the server `SRC_HASH` at runtime and confirm the narrower definition is still correct for those consumers, or record why it is.

The server build also produces the `test-agent` and `xccdf-export-fixture` binaries. Confirm the filtered closure still contains everything those binaries need to compile.

## Context

Read doc-23, `Build Invalidation Graph and CI Feedback Latency Analysis`, for the surrounding design and the constraints that apply across this initiative.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The server derivation input contains workspace metadata and only the local crates in the cf-server transitive closure
- [ ] #2 A source-only change under the builder crate does not change the server derivation hash, demonstrated by comparing derivation paths before and after the edit
- [ ] #3 A source-only change under the agent crate does not change the server derivation hash, demonstrated the same way
- [ ] #4 The server package still builds and still produces every binary it produced before, including test-agent and xccdf-export-fixture
- [ ] #5 The behavior and meaning of the server SRC_HASH after filtering is documented in the Nix source, including why the narrower hash is correct for its runtime consumers
- [ ] #6 Existing comments in packages/default/default.nix that describe server source filtering are accurate after the change, including the comment on the serverSrc binding
- [ ] #7 Checks that boot a server still pass
<!-- AC:END -->
