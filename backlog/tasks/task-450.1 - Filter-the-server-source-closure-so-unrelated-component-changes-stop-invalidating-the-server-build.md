---
id: TASK-450.1
title: >-
  Filter the server source closure so unrelated component changes stop
  invalidating the server build
status: Done
assignee: []
created_date: '2026-08-31 22:38'
updated_date: '2026-09-01 03:44'
labels: []
dependencies: []
references:
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/324'
documentation:
  - >-
    backlog/docs/build/build-invalidation-graph/doc-23 -
    Build-Invalidation-Graph-and-CI-Feedback-Latency-Analysis.md
modified_files:
  - packages/default/default.nix
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

## Non-goals

- Changing which binaries the server derivation produces.
- Changing the `mkWorkspaceSrc` or `mkComponentWorkspaceManifest` helpers for the agent, builder, or keygen components.
- Changing the meaning of `SRC_HASH` for the agent.
- Converting the server to a different Rust build framework. That is a separate subtask.

## Architectural constraints

- Follow the existing `mkWorkspaceSrc` and `mkComponentWorkspaceManifest` pattern rather than introducing a new filtering mechanism.
- The workspace manifest substituted into the build tree must list exactly the crates in the component closure, because Cargo parses every workspace member even when `--package` selects one.
- Migrations referenced by SQLx compile-time verification must remain inside the filtered source tree.

## Verification plan

- Compare `nix path-info --derivation` output for the server package before and after a scratch edit under `crates/cf-builder` and under `crates/cf-agent`.
- `nix build .#packages.x86_64-linux.server --no-link`.
- List the binaries in the build result and compare against the pre-change list.
- Build the checks that boot a server.

## Impact areas

`packages/default/default.nix`, the server derivation input, `SRC_HASH` embedded in the server binary, and every check or module that consumes the server package.

## Risk level

Low. The change is confined to one Nix expression and the failure mode is a loud build error rather than silent misbehavior. The one non-obvious risk is `SRC_HASH` semantics.

## Dependencies

None.

## Context

Read doc-23, `Build Invalidation Graph and CI Feedback Latency Analysis`, for the surrounding design and the constraints that apply across this initiative.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 The server derivation input contains workspace metadata and only the local crates in the cf-server transitive closure
- [x] #2 A source-only change under the builder crate does not change the server derivation hash, demonstrated by comparing derivation paths before and after the edit
- [x] #3 A source-only change under the agent crate does not change the server derivation hash, demonstrated the same way
- [x] #4 The server package still builds and still produces every binary it produced before, including test-agent and xccdf-export-fixture
- [x] #5 The behavior and meaning of the server SRC_HASH after filtering is documented in the Nix source, including why the narrower hash is correct for its runtime consumers
- [x] #6 Existing comments in packages/default/default.nix that describe server source filtering are accurate after the change, including the comment on the serverSrc binding
- [x] #7 Checks that boot a server still pass
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
LOCK: opencode-claude-opus-5 on gray in /home/mcamp/code/crystal-forge/TASK-450-p0-build-graph

Implemented together with TASK-450.2, TASK-450.3, and TASK-450.4 in a single MR at the user's explicit direction.

Verification: the original unfiltered expression changed cf-server from cyn1qw4a… to p6ibfini… after a builder edit. The filtered expression kept cf-server unchanged. Final probe kept both embedded and core server derivations unchanged after simultaneous builder and agent edits.

Verification passed: server package build, integration, oidc-auth, xccdf-schema, web-ui, test-agent NixOS system build, and one complete nix flake check --keep-going -L run. TASK-451 tracks the discovered duplicate SQLx cache.

LOCK RELEASED: implementation is pushed and MR !324 is awaiting review.

MR !324 merged. LOCK RELEASED and the dedicated P0 worktree was removed.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Filtered the server source closure to the cf-server transitive local-crate closure, workspace metadata, and required workspace-root SQLx metadata. Added a matching component workspace manifest and documented the narrowed SRC_HASH semantics. Builder and agent edits no longer invalidate either server variant.
<!-- SECTION:FINAL_SUMMARY:END -->
