---
id: TASK-450.2
title: >-
  Depend on exact component derivations instead of aggregate packages in
  modules, checks, and lib helpers
status: To Do
assignee: []
created_date: '2026-08-31 22:39'
labels: []
dependencies: []
documentation:
  - >-
    backlog/docs/build/build-invalidation-graph/doc-23 -
    Build-Invalidation-Graph-and-CI-Feedback-Latency-Analysis.md
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

## Constraints

The aggregate outputs must keep existing and keep resolving. They are public compatibility surfaces consumed through `flake.nix:41-45` and by `systems/x86_64-linux/test-agent/default.nix`. This task narrows internal usage only; it does not remove outputs.

Do not narrow a VM closure without first confirming which binaries that VM actually runs. Removing a binary a test invokes turns a fast check into a confusing runtime failure. Where a VM genuinely needs several components, list those components explicitly rather than reintroducing an aggregate.

## Context

Read doc-23, `Build Invalidation Graph and CI Feedback Latency Analysis`, for the surrounding design and the constraints that apply across this initiative.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The NixOS module references the exact component derivation for each service and helper script it defines, rather than an aggregate package
- [ ] #2 Every check and lib test helper references only the component derivations the corresponding VM actually executes
- [ ] #3 A repository search shows no remaining internal use of the aggregate packages outside the package definition itself and the public flake outputs
- [ ] #4 Aggregate flake outputs still exist and still evaluate, and external consumers such as the test-agent system configuration still build
- [ ] #5 Each narrowed closure records, in the Nix source, which binaries the consumer runs and therefore why that component set is sufficient
- [ ] #6 The integration, oidc-auth, web-ui, and xccdf-schema checks still pass
- [ ] #7 A recorded closure-size comparison shows the reduction for at least the server service and the integration check
<!-- AC:END -->
