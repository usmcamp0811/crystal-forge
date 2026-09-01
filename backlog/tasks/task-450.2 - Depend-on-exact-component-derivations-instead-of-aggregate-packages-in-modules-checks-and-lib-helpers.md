---
id: TASK-450.2
title: >-
  Depend on exact component derivations instead of aggregate packages in
  modules, checks, and lib helpers
status: Done
assignee: []
created_date: '2026-08-31 22:39'
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
  - modules/nixos/crystal-forge/default.nix
  - checks/integration/default.nix
  - checks/oidc-auth/default.nix
  - checks/web-ui/default.nix
  - checks/xccdf-schema/default.nix
  - lib/default.nix
  - lib/server-test-node/default.nix
  - packages/dev-env/composition.nix
  - packages/devScripts/default.nix
  - packages/run-postgres-jobs/default.nix
  - systems/x86_64-linux/cf-test-sys/default.nix
  - systems/x86_64-linux/test-agent/default.nix
parent_task_id: TASK-450
priority: high
type: enhancement
ordinal: 2000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

Internal consumers depend on aggregate packages rather than the component they actually use. The aggregates are `symlinkJoin` outputs: `crystal-forge` joins all four component derivations, and `server` joins the server, builder, and keygen (`packages/default/default.nix:241` and `packages/default/default.nix:301`).

Confirmed internal consumers of aggregates:

- `modules/nixos/crystal-forge/default.nix:341,406,411,438` use `pkgs.crystal-forge.default.server`
- `modules/nixos/crystal-forge/default.nix:443` uses `pkgs.crystal-forge.default.agent`
- `checks/integration/default.nix:21,82` include `pkgs.crystal-forge.default`, although `checks/integration/default.nix:126` sets `build.enable = false`
- `checks/web-ui/default.nix:135,189` include `pkgs.crystal-forge.default`
- `checks/xccdf-schema/default.nix:5` uses `pkgs.crystal-forge.default.server`
- `lib/default.nix:128,467` and `lib/server-test-node/default.nix:180` include `crystal-forge.default`

The effect is that a host or test VM that needs only the server pulls a closure containing the builder, the agent, and the key generator. Any change to any component invalidates that closure and forces rebuilds and store copies that the consumer does not need.

## Goal

Point every internal consumer at the exact derivation it executes, so component closures reflect real dependencies.

## Non-goals

- Removing, renaming, or changing the contents of any aggregate output.
- Changing the set of binaries any service or test actually runs.
- Changing NixOS module option names or their user-facing semantics.
- Restructuring the checks beyond their package references.

## Architectural constraints

- The aggregate outputs must keep existing and keep resolving. They are public compatibility surfaces consumed through `flake.nix:41-45` and by `systems/x86_64-linux/test-agent/default.nix`. This task narrows internal usage only; it does not remove outputs.
- Do not narrow a VM closure without first confirming which binaries that VM actually runs. Removing a binary a test invokes turns a fast check into a confusing runtime failure.
- Where a consumer genuinely needs several components, list those components explicitly rather than reintroducing an aggregate.
- `cf-keygen` is invoked by service preStart scripts and by test setup. Trace those call sites before assuming a service needs only the server binary.

## Verification plan

- Repository search proving no internal aggregate references remain outside the package definition and the public flake outputs.
- Build the NixOS module through a check that instantiates the services.
- Build `checks/integration`, `checks/oidc-auth`, `checks/web-ui`, and `checks/xccdf-schema`.
- Compare `nix path-info --closure-size` for the server service package and the integration check before and after.
- Build the `test-agent` system configuration to prove the public aggregates still resolve.

## Impact areas

`modules/nixos/crystal-forge/default.nix`, `checks/integration`, `checks/web-ui`, `checks/xccdf-schema`, `lib/default.nix`, `lib/server-test-node/default.nix`, and `packages/dev-env/composition.nix`.

## Risk level

Medium. Each individual substitution is mechanical, but an incorrect narrowing produces a missing-binary failure at VM runtime rather than at evaluation time, which is slower to diagnose.

## Dependencies

None.

## Context

Read doc-23, `Build Invalidation Graph and CI Feedback Latency Analysis`, for the surrounding design and the constraints that apply across this initiative.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 The NixOS module references the exact component derivation for each service and helper script it defines, rather than an aggregate package
- [x] #2 Every check and lib test helper references only the component derivations the corresponding VM actually executes
- [x] #3 A repository search shows no remaining internal use of the aggregate packages outside the package definition itself and the public flake outputs
- [x] #4 Aggregate flake outputs still exist and still evaluate, and external consumers such as the test-agent system configuration still build
- [x] #5 Each narrowed closure records, in the Nix source, which binaries the consumer runs and therefore why that component set is sufficient
- [x] #6 The integration, oidc-auth, web-ui, and xccdf-schema checks still pass
- [x] #7 A recorded closure-size comparison shows the reduction for at least the server service and the integration check
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
LOCK: opencode-claude-opus-5 on gray in /home/mcamp/code/crystal-forge/TASK-450-p0-build-graph

Implemented together with TASK-450.1, TASK-450.3, and TASK-450.4 in a single MR at the user's explicit direction.

Closure measurements: production server dependency closure 136.0 MiB to 125.7 MiB (-7.6%); integration derivation closure 705.2 MiB to 605.6 MiB (-14.1%).

Final invalidation probes: web UI edits leave cf-test-sys, integration, and oidc-auth derivations unchanged; builder edits leave integration and oidc-auth unchanged while correctly changing web-ui, which runs a builder.

Verification passed: integration, oidc-auth, xccdf-schema, web-ui, test-agent NixOS system build, and one complete nix flake check --keep-going -L run.

LOCK RELEASED: implementation is pushed and MR !324 is awaiting review.

MR !324 merged. LOCK RELEASED and the dedicated P0 worktree was removed.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Added exact package options for server, builder, and agent services and changed internal modules, checks, helpers, development tooling, and systems to use only the component derivations they execute. Preserved public aggregate outputs. Removed residual whole-flake invalidation from cf-test-sys and run-postgres-jobs.
<!-- SECTION:FINAL_SUMMARY:END -->
