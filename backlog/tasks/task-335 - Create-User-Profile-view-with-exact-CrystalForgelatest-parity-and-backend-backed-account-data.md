---
id: TASK-335
title: Create User Profile view (new route+view) with CrystalForgelatest parity
status: To Do
assignee: []
created_date: '2026-05-31 16:02'
updated_date: '2026-06-25 21:50'
labels:
  - design-parity
  - user-profile
  - web-ui
  - api-integration
milestone: m-20
dependencies:
  - TASK-328
  - TASK-329
references:
  - /home/mcamp/code/crystal-forge/CrystalForgelatest/components/ProfileView.jsx
  - packages/web-ui/src/routes.rs
  - packages/web-ui/src/views/mod.rs
  - design/doc-14 - Parity-execution-playbook-agent-proof.md
documentation:
  - design/doc-14 - Parity-execution-playbook-agent-proof.md
modified_files:
  - packages/web-ui/src/views/profile.rs
  - packages/web-ui/src/views/mod.rs
  - packages/web-ui/src/routes.rs
  - packages/web-ui/src/components/layout/sidebar.rs
  - packages/web-ui/src/api/models.rs
  - packages/web-ui/src/api/client.rs
  - checks/web-ui/tests/integration-test.js
priority: high
ordinal: 1680
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem Statement
There is currently no Profile route or view in the repo today, while the CrystalForgelatest design includes `components/ProfileView.jsx` with Profile & Preferences content. Users need a first-class Profile surface reachable from the shell user block.

## Goal
Create the Profile surface from scratch with CrystalForgelatest parity and backend-backed account data, including account information, preferences, and security/session sections where supported by existing APIs.

See `design/doc-14 - Parity-execution-playbook-agent-proof.md` for the standard parity execution procedure.

## Explicit Non-Goals
- No global auth architecture rework beyond what the profile surface needs.
- No broad user-management/admin redesign.
- No unrelated shell/sidebar redesign beyond making the existing user block navigate to Profile.
- No persistence of preferences that do not have an existing backend contract unless a minimal endpoint is clearly required by the profile scope; missing backend support should be captured as a follow-up if it cannot be implemented safely in scope.

## Exact Scope
1. Create `packages/web-ui/src/views/profile.rs` with a `ProfileView` component.
2. Register the module in `packages/web-ui/src/views/mod.rs`.
3. Add a `ProfileView` route in `packages/web-ui/src/routes.rs` (for example `/profile`) and a title entry.
4. Wire the sidebar user block at the bottom of `components/layout/sidebar.rs` to navigate to Profile, matching `Shell.jsx` behavior.
5. Implement sections/controls from `ProfileView.jsx`: account info, preferences, and security/session as applicable.
6. Back profile/account content with the real API client; add minimal endpoints only if missing and needed for the scoped view.
7. Implement edit/save/cancel/validation feedback plus loading, empty, error, and success states.
8. Extend the web-ui check with Profile coverage for loading, populated, and editing states plus at least one real interaction assertion.

## Architectural Constraints
- Follow existing Dioxus view/component patterns in `packages/web-ui`.
- Keep view rendering separate from API models/client code.
- UI code must not import infrastructure/server internals directly.
- Do not place business logic in the view; keep validation/state transitions small and localized or extracted if they grow.
- Preserve existing auth and role-gating behavior.
- Follow the parity workflow in `design/doc-14 - Parity-execution-playbook-agent-proof.md`.

## Impact Areas
- `packages/web-ui/src/views/profile.rs` (new)
- `packages/web-ui/src/views/mod.rs`
- `packages/web-ui/src/routes.rs`
- `packages/web-ui/src/components/layout/sidebar.rs`
- `packages/web-ui/src/api/models.rs` and `packages/web-ui/src/api/client.rs` if profile API support is needed
- `checks/web-ui/tests/integration-test.js`

## Risk Level
Medium: this introduces a new routed UI surface and may require profile/account API integration, but the scope is constrained to web-ui Profile parity and minimal supporting client/API changes.

## Dependencies
- `TASK-328` completed: parity matrix/spec foundation.
- `TASK-329` completed: shell/tokens/topbar/sidebar parity foundation.
- `TASK-333` is not an execution blocker for this task because its own notes place full strict parity harness closure in the final-audit milestone; this task must still add its own profile-specific web-ui screenshot/assertion coverage.

## Verification Plan
- `nix develop -c cargo fmt -- --check`
- `nix develop -c cargo check --manifest-path packages/web-ui/Cargo.toml --target wasm32-unknown-unknown`
- `nix build .#checks.x86_64-linux.web-ui` with a new profile step that captures required states and asserts a real interaction.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 New ProfileView route and view module exist and are wired in routes.rs and views/mod.rs
- [ ] #2 Sidebar user block navigates to the Profile view as in the design
- [ ] #3 Profile layout/controls match ProfileView.jsx across supported breakpoints
- [ ] #4 Edit/save/cancel/validation feedback works and profile/account content is backend-driven
- [ ] #5 web-ui check captures profile loading, populated, and editing states and asserts a real interaction
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Inspect the CrystalForgelatest ProfileView.jsx and current web-ui route/view/layout/API patterns.
2. Add and wire the Profile route/view module and sidebar user-block navigation.
3. Implement Profile & Preferences sections using backend-backed profile/account data and scoped edit/save/cancel validation states.
4. Add or reuse minimal API client/model support required for account/profile data without changing global auth architecture.
5. Extend the web-ui check to cover Profile loading/populated/editing states and assert at least one real interaction.
6. Run the declared formatting, wasm cargo check, and web-ui check verification commands.
<!-- SECTION:PLAN:END -->
