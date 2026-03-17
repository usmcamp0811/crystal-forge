---
id: TASK-192
title: Validate cache destination input in cache modal
status: To Do
assignee: []
created_date: '2026-03-14 16:49'
updated_date: '2026-03-17 00:12'
labels:
  - frontend
  - validation
  - cache
  - ux
dependencies: []
references:
  - packages/web-ui/src/views/caches.rs
priority: medium
ordinal: 5000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem
The cache destination modal currently accepts malformed or garbage destination input, which can lead to invalid cache destination records and confusing failures later in the deployment flow.

## Goal
Add client-side validation to the cache destination modal so only syntactically valid destination values can be submitted, with clear inline guidance when input is invalid.

## Non-Goals
- This task does NOT redesign the cache destination modal.
- This task does NOT add server-side validation beyond what already exists.
- This task does NOT change cache push execution behavior.
- This task does NOT add support for entirely new cache destination types.

## Scope
1. Validate destination input before submit.
2. Show inline validation messaging tied to the current cache type/value.
3. Preserve successful create behavior for valid inputs.
4. Cover malformed and valid cases for supported cache destination types.

## Architectural Constraints
- Keep validation logic close to the cache modal UI and separate from unrelated cache management logic.
- Validation messages should be actionable and specific to the malformed value.
- Reuse existing form-state patterns in `packages/web-ui/src/views/caches.rs` where possible.
- Do not introduce hidden global state for validation.

## Impact Areas
- `packages/web-ui/src/views/caches.rs`
- Supporting cache modal helpers/components only if needed for clarity/testability

## Risk Level
Low-Medium — focused frontend validation change with minimal backend risk.

## Verification Plan
- Tier 0:
  - `nix develop -c cargo fmt -- --check`
  - `nix develop -c cargo check --manifest-path packages/web-ui/Cargo.toml`
  - targeted frontend/unit tests if present for validation helpers
- Tier 1:
  - Run the web UI and manually verify invalid values are blocked with inline errors
  - Verify valid destination values still submit successfully
- Tier 2:
  - `nix flake check` not required for this scoped frontend validation task
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Cache destination modal rejects malformed destination input before submit.
- [ ] #2 Validation messages are shown inline and indicate what is wrong with the entered value.
- [ ] #3 Valid destination inputs for supported cache types are accepted.
- [ ] #4 Existing cache destination create flow remains functional for valid inputs.
- [ ] #5 Validation behavior covers malformed and valid examples for each supported cache destination type handled by the modal.
<!-- AC:END -->
