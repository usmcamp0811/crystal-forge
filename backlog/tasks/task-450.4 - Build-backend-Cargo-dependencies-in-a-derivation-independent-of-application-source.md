---
id: TASK-450.4
title: >-
  Build backend Cargo dependencies in a derivation independent of application
  source
status: Done
assignee: []
created_date: '2026-08-31 22:39'
updated_date: '2026-09-01 03:44'
labels: []
dependencies:
  - TASK-450.1
  - TASK-450.3
references:
  - 'https://crane.dev/API.html'
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/324'
documentation:
  - >-
    backlog/docs/build/build-invalidation-graph/doc-23 -
    Build-Invalidation-Graph-and-CI-Feedback-Latency-Analysis.md
modified_files:
  - flake.nix
  - flake.lock
  - packages/default/default.nix
parent_task_id: TASK-450
priority: high
type: enhancement
ordinal: 4000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

Every Rust package in the repository is built with `pkgs.rustPlatform.buildRustPackage`, which produces one derivation whose input includes the application source. Changing a single line in a `.rs` file changes the derivation hash and discards the compiled Cargo dependency tree from the previous build. There is no derivation whose input is limited to `Cargo.lock` and the workspace manifests.

The practical result for an agent iterating on backend code is that each attempt pays the full third-party crate compilation cost again.

## Goal

Introduce a dependency-only build artifact for the backend whose Nix input is the dependency metadata rather than the application source, and consume it from the backend server build.

`crane` provides `buildDepsOnly` for exactly this shape and is the intended mechanism. It is not currently an input of this flake.

## Scope

Convert the server first. The server is the dependency graph pulled into the most expensive checks, so it produces the largest measurable improvement and proves the approach.

Do not convert every Rust package in the repository in this task. `server-regressions` and the web UI are handled separately once the pattern is proven.

## Non-goals

- Converting the agent, builder, keygen, web UI, or `server-regressions` builds.
- Changing dependency versions or regenerating the lock file.
- Changing binary names, output names, or installed paths.
- Adding a binary cache. That is a separate subtask that depends on this one.

## Architectural constraints

- Reproducibility must not regress. Binaries must be built from the same locked dependency versions as today, and the build must remain offline and hermetic in the Nix sandbox.
- The change must compose with the source filtering and the core-versus-embedded-UI split from the sibling subtasks. Sequence the work so the dependency artifact is shared rather than duplicated per server variant.
- Existing package output names, binary names, and installed paths must not change.
- SQLx compile-time verification must continue to work, including its offline metadata and migration inputs.
- The `crane` input must be pinned in the flake lock like any other input.

## Verification plan

- Compare the dependency artifact derivation path before and after a scratch edit to a backend `.rs` file.
- `nix build .#packages.x86_64-linux.server --no-link` and compare the produced binary list against the pre-change list.
- Time a one-line backend source rebuild before and after, on an otherwise warm store, and record both numbers.
- Build the checks that build or boot the server.

## Impact areas

`flake.nix`, `flake.lock`, `packages/default/default.nix`, and every consumer of the server package.

## Risk level

Medium to high. This replaces the build mechanism for the most depended-upon component. The likely failure modes are a broken SQLx offline build, a missing build input that `buildRustPackage` previously supplied implicitly, and feature unification differences between the dependency build and the application build.

## Dependencies

Requires the server source filtering and the core-versus-embedded-UI split, so the dependency artifact is defined once against the final source shape and shared by both server variants.

## Context

Read doc-23, `Build Invalidation Graph and CI Feedback Latency Analysis`, for the surrounding design and the constraints that apply across this initiative.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 A backend dependency artifact is produced by a derivation whose input is workspace dependency metadata and not application source
- [x] #2 A source-only change to a backend crate does not change the hash of the dependency artifact derivation, demonstrated by comparing derivation paths before and after the edit
- [x] #3 The backend server build consumes the shared dependency artifact instead of recompiling third-party crates
- [x] #4 The server package produces the same binaries with the same names and paths as before the change
- [x] #5 Dependency versions still come from the committed lock file and the build remains hermetic inside the Nix sandbox
- [x] #6 A recorded measurement compares wall-clock rebuild time for a one-line backend source change before and after the change, on an otherwise warm store
- [x] #7 Checks that build or boot the server still pass
- [x] #8 The Nix source documents which inputs are intentionally excluded from the dependency artifact and why, so a future edit does not reintroduce an application-source dependency
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
LOCK: opencode-claude-opus-5 on gray in /home/mcamp/code/crystal-forge/TASK-450-p0-build-graph

Implemented together with TASK-450.1, TASK-450.2, and TASK-450.3 in a single MR at the user's explicit direction. The declared dependencies on TASK-450.1 and TASK-450.3 are satisfied within this MR by landing the source filter and the server variant split before the dependency-artifact conversion.

Crane input pinned at revision 692f7e9ef2ece8125b466f66f2af532b3edaed0d. The shared dependency artifact derivation is mav536fx…-cf-server-deps-0.3.0.drv and remained unchanged after a server .rs edit.

Controlled source-only rebuild on the same filtered/core code: buildRustPackage at 7d39b700 took 8m55.77s; Crane with a warm dependency artifact took 5m39.69s, a 3m16.08s / 36.6% reduction. The Crane log compiled only cf-protocol, cf-config, and cf-server.

Both server variants built, ran the existing test scope (over 1,500 tests including ignored entries), and installed server, hardening-worker, test-agent, and xccdf-export-fixture. The production aggregate retained its historical builder and cf-keygen binaries.

Verification passed: server package, test-agent/core package, all affected checks, test-agent NixOS system, and one complete nix flake check --keep-going -L run.

LOCK RELEASED: implementation is pushed and MR !324 is awaiting review.

MR !324 merged. LOCK RELEASED and the dedicated P0 worktree was removed.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Pinned Crane and added one source-independent cf-server dependency artifact based on Cargo.lock and cleaned manifests. Both core and embedded server builds reuse it while retaining locked, hermetic builds, SQLx offline compilation, existing tests, and installed binary paths. A source-only rebuild improved by 36.6% locally.
<!-- SECTION:FINAL_SUMMARY:END -->
