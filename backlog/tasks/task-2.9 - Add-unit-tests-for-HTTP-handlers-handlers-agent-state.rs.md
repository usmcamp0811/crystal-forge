---
id: TASK-2.9
title: Add unit tests for HTTP handlers - handlers/agent/state.rs
status: In Progress
assignee: ["Codex 5.3"]
created_date: '2026-02-04 20:39'
updated_date: '2026-02-20 02:10'
labels:
  - testing
  - handlers
  - http
milestone: m-1
dependencies: []
parent_task_id: TASK-2
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Test request validation and state update logic.
<!-- SECTION:DESCRIPTION:END -->

## Problem Statement

`handlers/agent/state.rs` lacks focused unit tests for request validation and response behavior, leaving key handler behavior unverified.

## Goal

Add unit tests for the handler logic in `handlers/agent/state.rs` so validation, compatibility handling, serialization, and service interactions are covered and deterministic.

## Non-Goals

- Do not refactor unrelated handlers or routing setup.
- Do not change API contracts beyond what tests require.
- Do not alter unrelated deployment or builder logic.

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Test valid state update request
- [x] #2 Test invalid payload format
- [x] #3 Test version compatibility (V1 vs current)
- [x] #4 Test response serialization
- [x] #5 Mock agent service calls
<!-- AC:END -->

## Architectural Constraints

- Keep business logic out of HTTP view/transport code.
- Follow existing handler test patterns in the repository.
- No infrastructure coupling from handler tests beyond required mocking seams.

## Verification Plan

Automated:

- `nix develop -c cargo test --manifest-path packages/default/Cargo.toml handlers::agent::state`
- `nix develop -c cargo test --manifest-path packages/default/Cargo.toml`
- `nix develop -c cargo clippy --manifest-path packages/default/Cargo.toml -- -D warnings`
- `nix develop -c cargo fmt --manifest-path packages/default/Cargo.toml -- --check`

Manual:

- Review test cases in `handlers/agent/state.rs` and confirm each acceptance criterion maps to at least one assertion.

## Impact Areas

- API handler tests
- Rust test suite reliability

## Risk Level

Low - test-only changes scoped to a single handler module.

## Dependencies

None.

## Implementation Notes

LOCK: OpenCode on gray in /home/mcamp/code/crystal-forge/TASK-2.9-agent-state-handler-tests

Added a testable handler core (`update_with_lookup_and_insert`) with injected lookup/insert dependencies, then covered success, invalid payload, version compatibility (current vs V1), and insert failure paths in unit tests.

Verification run:
- `SQLX_OFFLINE=true nix develop -c cargo test --manifest-path packages/default/Cargo.toml handlers::agent::state` (pass)

Verification issues observed:
- `nix develop -c cargo fmt --manifest-path packages/default/Cargo.toml -- --check` fails due pre-existing unrelated formatting diffs in repository.
- `SQLX_OFFLINE=true nix develop -c cargo clippy --manifest-path packages/default/Cargo.toml -- -D warnings` fails due existing repository-wide warnings and a local target toolchain mismatch requiring clean rebuild.
