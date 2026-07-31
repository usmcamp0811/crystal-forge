---
id: TASK-375.2
title: Stream derivation archive responses for API builders
status: Backlog
assignee: []
created_date: '2026-06-29 22:47'
labels:
  - builder
  - api
  - performance
milestone: Builder API hotfix
dependencies:
  - TASK-375
references:
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/289'
modified_files:
  - packages/default/src/handlers/api/builders.rs
  - packages/default/src/builder/api_client.rs
priority: medium
ordinal: 0
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Problem: the API builder derivation archive endpoint currently buffers concatenated `nix-store --export` output in server memory before responding. Production validation of TASK-375 showed server RSS jumping to multiple GiB while exporting a large NixOS closure (~34k paths). This is acceptable only as a short-term hotfix and risks memory pressure or OOM on large closures.

Desired Outcome: derivation archive downloads stream from the server to the builder without buffering the whole NAR payload in memory, while preserving chunked export behavior that avoids ARG_MAX.

Non-Goals:
- Do not change builder scheduling semantics.
- Do not reintroduce builder direct database access.
- Do not replace the server-mediated archive transfer design.

Impact Areas:
- packages/default/src/handlers/api/builders.rs
- packages/default/src/builder/api_client.rs
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Derivation archive endpoint streams archive bytes instead of collecting the whole response in a Vec before sending.
- [ ] #2 Chunked nix-store export behavior remains in place so large path sets do not exceed ARG_MAX.
- [ ] #3 Remote API builders can import streamed archives without direct DB or shared /nix/store access.
- [ ] #4 Large NixOS derivation archive download does not cause multi-GiB server RSS growth attributable to response buffering.
<!-- AC:END -->
