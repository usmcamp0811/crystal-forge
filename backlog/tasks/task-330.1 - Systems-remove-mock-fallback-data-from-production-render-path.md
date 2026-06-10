---
id: TASK-330.1
title: 'Systems: remove mock/fallback data from production render path'
status: Backlog
assignee: []
created_date: '2026-06-10 13:28'
labels:
  - design-parity
  - systems
  - api-integration
  - child
milestone: 'm-19: Design Parity Existing Surfaces'
dependencies: []
references:
  - TASK-330
  - packages/web-ui/src/views/systems_list.rs
  - packages/web-ui/src/systems/adapter.rs
  - design/doc-14 - Parity-execution-playbook-agent-proof.md
documentation:
  - design/doc-14 - Parity-execution-playbook-agent-proof.md
modified_files:
  - packages/web-ui/src/views/systems_list.rs
  - packages/web-ui/src/systems/adapter.rs
  - checks/web-ui/tests/integration-test.js
parent_task_id: TASK-330
priority: high
ordinal: 1621
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Child of Systems umbrella TASK-330. Follow guide doc-14 standard procedure.

## Problem
`packages/web-ui/src/views/systems_list.rs` uses `load_systems_with_fallback` and `fallback_systems` and shows a "using mock data" notice banner (around lines 160, 173, 327). Production rendering must reflect real backend truth only.

## Goal
Make the Systems list render exclusively from the real API in the production path, with proper loading/empty/error states instead of mock fallback.

## Exact scope
1. Replace `load_systems_with_fallback(...)` usage in the production render path with the real API call, surfacing a real error state on failure.
2. Remove the `local_systems` mock seeding via `fallback_systems` from the production path (keep fallbacks only behind dev/test-only gates if they are still needed for AUTH_MODE=dev; otherwise remove).
3. Remove the "API fallback notice banner" mock-data messaging (around line 327) and replace with a real empty/error state per design.
4. Confirm `views/systems_mock*.rs` are not referenced by the production path (coordinate with TASK-341 if deletions are needed).

## Non-goals
- No layout redesign here (covered by sibling Systems tasks).
- No backend endpoint changes unless a required field is missing (note it for TASK-332 if so).

## Files
- packages/web-ui/src/views/systems_list.rs
- packages/web-ui/src/systems/adapter.rs
- checks/web-ui/tests/integration-test.js

## Verification
- nix develop -c cargo fmt -- --check
- nix develop -c cargo check --manifest-path packages/web-ui/Cargo.toml --target wasm32-unknown-unknown
- nix build .#checks.x86_64-linux.web-ui
- Reuse/extend existing step `12d-systems-api-error-no-mock-fallback` to assert no mock data renders on API error.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Systems list production path renders only from the real API (no fallback_systems seeding)
- [ ] #2 API failure shows a real error state, not mock data or a mock-data notice banner
- [ ] #3 Empty state matches the design when the API returns zero systems
- [ ] #4 web-ui step asserts no mock rows render on API error
- [ ] #5 cargo fmt, web-ui cargo check (wasm), and nix build .#checks.x86_64-linux.web-ui pass
<!-- AC:END -->
