---
id: TASK-375.3
title: 'Remote builders: replace archive bootstrap with pull-based store transport'
status: To Do
assignee: []
created_date: '2026-06-30 17:46'
labels:
  - builder
  - remote-builds
  - architecture
  - nix
  - hotfix-followup
milestone: Builder API hotfix
dependencies:
  - TASK-375
references:
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/289'
modified_files:
  - packages/default/src/handlers/api/builders.rs
  - packages/default/src/models/builders.rs
  - packages/default/src/builder/api_client.rs
  - packages/default/src/bin/builder.rs
  - packages/default/src/queries/builders.rs
  - modules/nixos/crystal-forge/default.nix
parent_task_id: TASK-375
priority: high
ordinal: 5510
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Problem: TASK-375 stabilized API-only remote builders, but the current server-derivation transport still relies on synchronous cache publication and a giant derivation-closure archive fallback. Large NixOS system closures can contain tens of thousands of paths, causing ARG_MAX pressure, multi-GiB memory buffering risk, long critical-path Attic pushes, and fragile custom failure semantics before the actual build starts.

Goal: keep `server_derivation` as Crystal Forge's authoritative production remote-builder strategy while replacing the monolithic archive/bootstrap path with Nix-native, pull-based store/substituter transport. Builders should realize the server-authorized `.drv` and pull missing paths individually from approved substituters, including a Crystal Forge store/substituter endpoint, without requiring a shared `/nix/store` or direct database access.

Non-Goals:
- Do not switch production remote builders to source checkout/re-evaluation in this task.
- Do not silently fall back between execution strategies.
- Do not reintroduce builder database access.
- Do not require every job to synchronously push the full derivation closure to Attic before assignment.
- Do not solve output reproducibility attestation beyond preserving the server-authorized derivation identity.

Architectural Constraints:
- Server remains authoritative for evaluation, policy, and expected derivation identity.
- Builder remains API-only and job-scoped.
- Transport must be path-oriented and streaming; avoid buffering whole multi-path archives in Rust memory.
- Use standard Nix substituter/remote-store semantics where practical: narinfo, NAR streaming, references, signatures, and trusted public keys.
- Attic should be treated as durable/asynchronous cache infrastructure, not mandatory synchronous job bootstrap orchestration.
- Job/attempt state must distinguish materialization failures from actual build failures.

Impact Areas:
- Builder job manifest/API models
- Builder runtime configuration for substituters/trusted keys
- Server-side store/substituter endpoint or remote-store bridge
- Build attempt phase/state tracking
- Cache publication queue behavior
- Documentation and operator guidance

Risk Level: high

Verification Plan:
- Unit tests for path metadata/narinfo generation and authorization checks.
- Integration test with a remote/API builder that realizes a server-evaluated `.drv` by pulling missing paths through the new transport.
- Failure-path test where Crystal Forge store endpoint or substituter is unavailable and the job enters a terminal `path_materialization_failed`/retryable attempt state rather than staying `building`.
- Confirm builder has no DB connection and no source/Git credentials in `server_derivation` mode.
- Confirm large closure bootstrap streams per path and does not allocate a whole archive in server memory.
- Targeted `nix develop` cargo checks/tests for changed crates; run heavier Nix checks only if Nix modules/packaging are modified.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 `server_derivation` remains the default authoritative production remote-builder strategy.
- [ ] #2 Remote builder can realize a server-authorized `.drv` without direct DB access and without a shared server `/nix/store`.
- [ ] #3 Builder pulls missing paths through configured substituters or a Crystal Forge path-oriented store endpoint instead of receiving one monolithic multi-path archive.
- [ ] #4 Transport streams per-path metadata/content and avoids buffering full closure archives in server memory.
- [ ] #5 Synchronous Attic closure publication is no longer required as the scheduler's critical path for job bootstrap.
- [ ] #6 Materialization failures are recorded as explicit terminal or retryable attempt failures and do not leave jobs stuck in `building`.
- [ ] #7 Operator documentation explains required builder substituters/trusted keys and the new failure phases.
<!-- AC:END -->
