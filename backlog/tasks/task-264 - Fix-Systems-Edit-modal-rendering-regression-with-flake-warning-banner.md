---
id: TASK-264
title: Fix Systems Edit modal rendering regression with flake warning banner
status: Review
assignee: []
created_date: '2026-04-11 16:00'
updated_date: '2026-04-11 16:20'
labels:
  - bug
  - ui
  - systems
  - modal
  - sprint-ready
milestone: Sprint
dependencies: []
references:
  - packages/web-ui/src/components/system/edit_system_modal.rs
  - packages/web-ui/src/views/systems_list.rs
  - checks/web-ui/tests/integration-test.js
priority: high
ordinal: 5100
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem Statement
On latest `dev`, opening **Systems → Edit** for an unlinked system (example: `nix-builder`) shows malformed modal behavior: global flake-link warning/banner content and surrounding page content visually leak into the modal flow instead of remaining outside a proper overlay.

## Goal
Restore correct modal overlay behavior for Systems Edit so only edit-form UI is inside the modal container and page-level warning/navigation content stays outside.

## Non-Goals
- No redesign of systems page layout.
- No changes to flake-link warning business rules or copy.
- No unrelated refactors to modal framework.

## Architectural Constraints
- Reuse existing modal overlay/container pattern already used by stable modals (e.g., Update Key).
- Keep warning banner rendering in page-level view composition, not inside modal subtree.
- Keep change scope to Systems view/edit modal wiring and required styling hooks only.

## Verification Plan
- Tier 0:
  - `nix develop -c cargo check --manifest-path packages/web-ui/Cargo.toml`
- Tier 1 (UI behavior):
  - `nix build .#checks.x86_64-linux.web-ui -L --show-trace`
  - Verify screenshot/assertion coverage demonstrates Edit modal renders as isolated overlay with no page-content leakage.

## Impact Areas
- `packages/web-ui/src/components/system/edit_system_modal.rs`
- `packages/web-ui/src/views/systems_list.rs`
- `checks/web-ui/tests/integration-test.js` (if assertion updates required)

## Risk Level
Medium (user-facing modal layout/regression fix).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Clicking Systems → Edit opens an isolated modal overlay; global flake-link warning banner and page navigation/content do not render inside modal body.
- [x] #2 Edit modal remains functional (hostname/config/environment/policy fields and Save/Cancel actions still render and behave normally).
- [x] #3 Systems page warning banner remains visible in page context when applicable, outside modal overlay.
- [x] #4 web-ui verification captures the corrected modal behavior (via existing or updated check evidence).
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented modal overlay isolation fix in `packages/web-ui/src/components/system/edit_system_modal.rs` (bounded max-height + internal scroll + fixed backdrop padding/overflow).

Added web-ui integration assertion in `checks/web-ui/tests/integration-test.js` step `12e-systems-edit-modal` to ensure warning banner text remains outside modal overlay.

Verification executed: `nix develop -c cargo check --manifest-path packages/web-ui/Cargo.toml` and `nix build .#checks.x86_64-linux.web-ui -L --show-trace`.

MR opened: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/229
<!-- SECTION:NOTES:END -->
