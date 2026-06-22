---
id: TASK-335
title: Create User Profile view (new route+view) with CrystalForgelatest parity
status: Backlog
assignee: []
created_date: '2026-05-31 16:02'
updated_date: '2026-06-21 02:08'
labels:
  - design-parity
  - user-profile
  - web-ui
  - api-integration
milestone: m-20
dependencies:
  - TASK-328
  - TASK-329
  - TASK-333
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
Problem: there is NO Profile route or view in the repo today (verified: not in routes.rs or views/mod.rs), but the design has `components/ProfileView.jsx`.

Goal: create the Profile surface from scratch with design parity and backend-backed account data.

See guide doc-14 for the standard procedure.

## Exact scope (new files + wiring)
1. Create `packages/web-ui/src/views/profile.rs` with a `ProfileView` component.
2. Register module in `packages/web-ui/src/views/mod.rs`.
3. Add a `ProfileView` route in `packages/web-ui/src/routes.rs` (e.g. `/profile`) and a title entry.
4. Wire the sidebar user block (bottom of `components/layout/sidebar.rs`) to navigate to Profile, matching `Shell.jsx`.
5. Implement sections/controls from `ProfileView.jsx`: account info, preferences, security/session as applicable.
6. Back the data with the real API client; add endpoints only if missing (and note them for a backend follow-up).
7. Implement edit/save/cancel/validation and loading/empty/error/success states.

## Non-goals
- No global auth architecture rework beyond what the profile surface needs.

## Files
- packages/web-ui/src/views/profile.rs (new)
- packages/web-ui/src/views/mod.rs
- packages/web-ui/src/routes.rs
- packages/web-ui/src/components/layout/sidebar.rs
- packages/web-ui/src/api/models.rs, client.rs (if needed)
- checks/web-ui/tests/integration-test.js (new step)

## Verification
- nix develop -c cargo fmt -- --check
- nix develop -c cargo check --manifest-path packages/web-ui/Cargo.toml --target wasm32-unknown-unknown
- nix build .#checks.x86_64-linux.web-ui (with a new profile step)
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 New ProfileView route and view module exist and are wired in routes.rs and views/mod.rs
- [ ] #2 Sidebar user block navigates to the Profile view as in the design
- [ ] #3 Profile layout/controls match ProfileView.jsx across supported breakpoints
- [ ] #4 Edit/save/cancel/validation feedback works and profile/account content is backend-driven
- [ ] #5 web-ui check captures profile loading, populated, and editing states and asserts a real interaction
<!-- AC:END -->
