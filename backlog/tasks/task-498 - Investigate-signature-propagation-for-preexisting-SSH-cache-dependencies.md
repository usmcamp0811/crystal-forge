---
id: TASK-498
title: Investigate signature propagation for preexisting SSH cache dependencies
status: Backlog
assignee: []
created_date: '2026-10-20 03:51'
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
A real CF builder recursively signed its local system closure and published successfully to a prepopulated test SSH store. Only three newly missing candidate paths transferred. The subsequent production agent copy failed because a preexisting baseline dependency at that destination lacked a signature trusted by the target. Top-level CF signature verification passed; the destination's whole closure was not readable with configured CF keys. Failed Delivery evidence /nix/store/cbxbrqalva8jm45crs4bx4gfhpmz0jm6-vm-test-run-cf-it-delivery-delivery-lifecycle-run records three retries and no activation. Determine and document the supported warm-cache signing guarantee; do not assume ordinary Nix copy merges signature metadata for valid paths. No repair is authorized by TASK-496. The bounded Delivery increment will share only immutable runtime prerequisites with targetA and continue to require cold runtime candidate acquisition; that is not full cold-closure acquisition coverage.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A reproduction distinguishes source recursive signing from signature metadata available for preexisting destination dependencies.
- [ ] #2 The supported publication and target-read trust contract for a warm SSH destination is documented.
- [ ] #3 If the contract requires signature propagation then a focused repair and regression demonstrate full-closure target acquisition without disabling signatures or trusting test-side receiver keys.
<!-- AC:END -->
