---
id: TASK-336.2
title: 'Admin Server: view parity + real server settings flows'
status: Review
assignee: []
created_date: '2026-06-20 02:19'
updated_date: '2026-06-20 15:34'
labels:
  - design-parity
  - admin
  - server
  - web-ui
  - child
milestone: 'm-20: Design Parity Missing Surfaces'
dependencies:
  - TASK-336
references:
  - TASK-336
  - /home/mcamp/code/crystal-forge/CrystalForgelatest
  - TASK-340.1
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/282'
  - TASK-336.3
  - TASK-336.4
  - TASK-336.5
  - TASK-336.6
  - TASK-336.7
  - TASK-336.8
documentation:
  - design/doc-13 - Sidebar-surface-execution-map.md
  - design/doc-14 - Parity-execution-playbook-agent-proof.md
modified_files:
  - packages/web-ui/src/views/admin.rs
parent_task_id: TASK-336
priority: high
ordinal: 1671
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Child of Admin sidebar surface umbrella TASK-336.

## Problem Statement
The Server/Admin server surface needs the same design-parity treatment as the other CrystalForgelatest parity tasks. The UI should match the Server/Admin reference surface exactly while preserving real backend-backed behavior where it already exists.

## Goal
Bring the Server view within the Admin surface to CrystalForgelatest design parity, including layout, cards, controls, dialogs, empty/loading/error states, and critical interactions. Where backend support is already simple and directly available, wire the UI to real API behavior. Where a design feature lacks backend/API support or requires non-trivial backend work, keep the UI visually faithful and clearly mark the specific control/value as not implemented yet with subtle text, then create a follow-up Backlog task for that backend/API feature.

## Explicit Non-Goals
- Do not implement unrelated Admin/IAM views beyond the Server surface.
- Do not redesign the global shell/sidebar/topbar.
- Do not introduce placeholder production behavior that appears functional but is not backend-backed.
- Do not silently drop or fake server configuration changes.
- Do not expand into complex backend/API work unless it is simple, directly scoped, and safe.

## Scope
1. Match the Server/Admin server view to the CrystalForgelatest reference design.
2. Preserve or add real backend-backed behavior for supported server settings/status/actions where simple.
3. For unsupported design fields/actions, render them per design but label them subtly as not implemented yet and create follow-up Backlog tasks.
4. Add or update web-ui coverage and screenshots for the Server surface, following the same parity-check pattern as recent surface parity tasks.
5. Keep UI logic separated from backend/API/data logic using existing project patterns.

## Architectural Constraints
- UI components should remain presentation/composition focused; business logic must not live in view rendering code.
- API models/DTOs should mirror server models and existing API client patterns.
- Backend changes must be minimal and directly required for Server view parity/round-trip behavior.
- If SQLx query shapes, migrations, or database schema change, SQLx metadata must be refreshed with the repo devshell workflow.
- If a design feature cannot be persisted or executed safely, do not fake it; show subtle not-implemented text and create a follow-up task.

## Impact Areas
- `packages/web-ui/src/views/admin.rs` and/or Server/Admin view components
- `packages/web-ui/src/api/**` if client DTOs are needed
- `packages/server/src/**` only for simple directly scoped backend support
- `checks/web-ui/tests/integration-test.js`
- potentially CSS/assets shared by Admin/Server UI

## Risk Level
Medium-high. The task touches an Admin/Server surface where incorrect placeholder controls could mislead operators. Main risks are scope creep, fake controls, backend/API mismatch, and expensive UI checks.

## Dependencies
- Parent umbrella: TASK-336. This is contextual only and is not an execution-blocking dependency.
- CrystalForgelatest reference files under `/home/mcamp/code/crystal-forge/CrystalForgelatest`.
- Existing parity task conventions from recent surface work such as TASK-340.1.

## Verification Plan
- Inspect the CrystalForgelatest Server/Admin reference and current `packages/web-ui/src/views/admin.rs` before implementation.
- Run targeted formatting for changed Rust files, preferably `nix develop -c rustfmt --edition 2024 --check <changed-files>` or the repo-equivalent targeted format command.
- Run `nix develop -c cargo check --manifest-path packages/web-ui/Cargo.toml --target wasm32-unknown-unknown` for web-ui compile validation.
- If backend/API code changes are made, run targeted compile/tests for the touched server/API crate(s).
- If SQLx query shapes or migrations change, use the repository devshell and local process-compose database flow to refresh SQLx metadata.
- Update `checks/web-ui/tests/integration-test.js` so the Server surface is asserted and screenshot-captured, and run the appropriate `web-ui` check before final review unless explicitly deferred by the owner.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Server/Admin server view visually matches the CrystalForgelatest reference for layout, spacing, typography, cards, controls, and states
- [x] #2 Supported server settings/status/actions are backed by real API data or real mutations with no fake production behavior
- [x] #3 Unsupported design fields/actions are still shown per design but clearly marked with subtle not-implemented text and have follow-up Backlog tasks created
- [x] #4 Simple directly scoped backend/API gaps required for real behavior are implemented when safe; non-trivial backend work is deferred to follow-up tasks
- [ ] #5 Server view loading, empty, error, populated, and key dialog/action states are covered by web-ui assertions/screenshots following existing parity-check patterns
- [x] #6 UI remains separated from backend/API/domain logic and follows existing Admin/web-ui architecture patterns
- [x] #7 No unsupported Server setting/action may appear to succeed unless it is actually persisted or executed by backend/API behavior
<!-- AC:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Review blockers addressed in MR !282 commit 1ab5cb1a: restored real-backed user create/edit/delete controls, replaced OIDC edit accidental delete with modal edit plus confirmed remove action, removed fabricated operational/security values, and wired Server heartbeat environment rows to real EnvironmentSummary data including system_count. Verification passed: nix develop -c bash -lc 'cd packages/web-ui && cargo fmt -- --check && cargo check --target wasm32-unknown-unknown'.
<!-- SECTION:FINAL_SUMMARY:END -->
