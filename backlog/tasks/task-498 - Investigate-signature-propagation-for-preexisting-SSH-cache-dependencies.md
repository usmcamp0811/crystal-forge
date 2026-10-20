---
id: TASK-498
title: Investigate signature propagation for preexisting SSH cache dependencies
status: In Progress
assignee:
  - openai
created_date: '2026-10-20 03:51'
updated_date: '2026-10-20 20:31'
labels: []
dependencies: []
references:
  - packages/default/crates/cf-config/src/config/cache.rs
  - packages/default/crates/cf-builder/src/derivations/build.rs
  - packages/default/crates/cf-agent/src/deployment/agent.rs
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/335'
priority: medium
type: bug
ordinal: 549000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
A successful completed publication to a populated supported destination must leave the required closure readable by a target using the configured CF trust. The real Delivery workflow showed recursive source signing and successful three-path transfer, followed by agent rejection of an already-valid destination dependency without a trusted signature. Source signing and destination metadata propagation are separate operations. The owner now authorizes the smallest supported warm-cache publication/readability repair and missing-dependency proof in the existing topology; do not trust receiver-test keys or disable signatures.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A reproduction distinguishes source recursive signing from signature metadata available for preexisting destination dependencies.
- [ ] #2 The supported publication and target-read trust contract for a warm SSH destination is documented.
- [ ] #3 If the contract requires signature propagation then a focused repair and regression demonstrate full-closure target acquisition without disabling signatures or trusting test-side receiver keys.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Dedicated worktree/branch TASK-498-publication-trust, clean base df11c800. Sol owns topology/registration and final integration; worker owns cf-config cache command construction and real Nix publication execution paths/tests and their adjacent contracts.
1. Inspect pinned Nix and supported Nix/SSH publication contract. Reproduce populated-destination copy skipping signatures on already valid paths, separately from source recursive signing.
2. Preserve recursive signing and normal transfer; explicitly synchronize the valid configured source signatures across the required destination closure using the supported pinned Nix operation. Publication completion must fail if required trust synchronization fails. No receiver-test key, disabled checks or privileged target import.
3. Use focused isolated-store command/regression tests and negative propagation-failure tests; no new public check/package or test-specific VM. Preserve other cache adapters' semantics and redaction.
4. Commit repair separately. Sol adds a missing-dependency acquisition proof inside the existing five-role Delivery lifecycle while retaining ordinary prepared activation and the original enforced receiver proof.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Owner authorization: current TASK-490 assignment selects TASK-498 for bounded production repair. Failed reproduction: /nix/store/cbxbrqalva8jm45crs4bx4gfhpmz0jm6-vm-test-run-cf-it-delivery-delivery-lifecycle-run. Ordinary activation may share immutable prerequisites but cannot replace the separate missing-dependency acquisition assertion. Dedicated product worktree from df11c800; coordinator owns integration topology/registration.
<!-- SECTION:NOTES:END -->
