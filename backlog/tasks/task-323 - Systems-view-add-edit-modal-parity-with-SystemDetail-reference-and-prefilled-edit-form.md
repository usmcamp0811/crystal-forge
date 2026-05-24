---
id: TASK-323
title: >-
  Systems view add/edit modal parity with SystemDetail reference and prefilled
  edit form
status: Backlog
assignee: []
created_date: '2026-05-24 16:28'
labels:
  - ui
  - ux
  - systems
  - modal
  - pixel-perfect
  - high-priority
  - sprint-ready
milestone: UI/UX Design System
dependencies: []
references:
  - >-
    /home/mcamp/code/crystal-forge/CrystalForgelatest/components/SystemDetail.jsx
modified_files:
  - packages/web-ui/src/views/systems.rs
  - packages/web-ui/src/components
  - packages/web-ui/src/api/models.rs
  - packages/default/src/handlers/api
priority: high
ordinal: 2200
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem
The Systems view add/edit modal does not match the design reference at `/home/mcamp/code/crystal-forge/CrystalForgelatest/components/SystemDetail.jsx`. In addition, opening the **Edit** modal for an existing system can present a blank/default form instead of the current system settings, which risks accidental misconfiguration and poor operator UX.

## Goal
Implement a pixel-accurate Systems add/edit modal aligned to `SystemDetail.jsx`, and ensure Edit mode is always pre-populated with the selected system’s current persisted settings.

## Non-Goals
- Do not redesign unrelated Systems list/table layout.
- Do not change backend domain semantics beyond what is required to load/save existing modal fields.
- Do not introduce new system configuration concepts not present in the reference component and current API contract.

## Architectural Constraints
- Keep business logic out of Dioxus view layer.
- Reuse existing DTO/API model patterns in `packages/web-ui/src/api/models.rs` and API client calls.
- Separate modal presentation from state/data-loading logic where practical.
- No unwraps in production code paths; explicit error handling for load/save failures.

## Verification Plan
- Tier 0 during implementation: targeted `cargo check`/tests for web-ui and relevant backend package.
- Tier 1 behavior verification for Systems modal open/edit/save paths.
- UI proof via `web-ui` check screenshots demonstrating add mode, edit mode (prefilled), and save flows.

## Impact Areas
- `packages/web-ui/src/views/systems*`
- `packages/web-ui/src/components/**/system*modal*`
- `packages/web-ui/src/api/models.rs` and client methods as needed
- Backend handlers/models only if required to return missing editable fields

## Risk Level
High — user-facing configuration workflow with high chance of regression if edit prefill/state sync is wrong.

## Dependencies
- None blocking identified; if API fields are missing, backend support must be added within this task scope.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Systems add/edit modal visual structure, spacing, labels, and interactions match the SystemDetail.jsx reference for all modal sections in scope.
- [ ] #2 Opening Edit for a specific system pre-populates every editable field with that system’s current persisted settings (no blank/default-only form in edit mode).
- [ ] #3 Add mode opens with defined create defaults only, and does not leak values from previous edit sessions.
- [ ] #4 Switching between editing different systems updates modal state to the newly selected system values without stale data.
- [ ] #5 Cancel/close actions do not persist unsaved changes; reopening Edit reloads persisted system values.
- [ ] #6 Save in Edit mode sends correct payload for changed and unchanged fields and persists successfully.
- [ ] #7 Validation and API error states are visible and non-destructive; form values are preserved on recoverable errors.
- [ ] #8 Successful save closes modal (or returns to expected post-save state) and refreshes systems view data.
- [ ] #9 Role/permission behavior for editing systems remains enforced (no privilege escalation via UI state).
- [ ] #10 `nix develop -c cargo check -p web-ui` passes.
- [ ] #11 `nix build .#checks.x86_64-linux.web-ui` passes and includes screenshot evidence showing: add modal, edit modal prefilled, and post-save state.
<!-- AC:END -->
