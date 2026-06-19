---
id: TASK-340.1
title: 'Policies: view + policy editor modal parity'
status: In Progress
assignee:
  - gpt-5.5
created_date: '2026-06-10 13:34'
updated_date: '2026-06-19 03:20'
labels:
  - design-parity
  - policies
  - web-ui
  - child
milestone: 'm-20: Design Parity Missing Surfaces'
dependencies: []
references:
  - TASK-340
  - >-
    /home/mcamp/code/crystal-forge/CrystalForgelatest/components/PoliciesView.jsx
  - design/doc-14 - Parity-execution-playbook-agent-proof.md
documentation:
  - design/doc-8 - CrystalForgelatest-UI-Parity-Matrix-TASK-328.md
  - design/doc-14 - Parity-execution-playbook-agent-proof.md
modified_files:
  - packages/web-ui/src/views/policies.rs
  - packages/web-ui/src/components/policy/mod.rs
  - checks/web-ui/tests/integration-test.js
parent_task_id: TASK-340
priority: high
ordinal: 1701
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Child of Policies umbrella TASK-340. Follow guide doc-14 standard procedure.

## Problem Statement
The Policies view (`views/policies.rs`, route `/deployment-policies`) does not yet match `CrystalForgelatest/components/PoliciesView.jsx`. The list presentation, chips, rule summaries, and policy editor workflows need parity with the design example. The editor must support the existing deployment policy capabilities, including basic policies, advanced policies, CVE-gate policies, and multi-rule policies.

## Goal
Bring the Policies list and policy editor modal to design parity while preserving real API-backed CRUD behavior. If parity work reveals missing backend/API support required for the UI to round-trip the supported policy shapes, this task may include small backend/API fixes that are directly necessary for the policy editor to function correctly.

## Explicit Non-Goals
- Do not implement the Compliance view; that remains separate surface TASK-344.
- Do not add unrelated policy types beyond the existing intended basic, advanced, CVE-gate, and multi-rule flows.
- Do not refactor unrelated views or design-system components unless the change is directly required for Policies parity.
- Do not alter deployment enforcement semantics beyond minimal API/model fixes needed for UI CRUD round-trip correctness.
- Do not redesign the global shell/sidebar/topbar surfaces.

## Scope
1. Policies list layout, chips, and rule summaries match the design example.
2. New/edit policy modal supports basic, advanced, CVE-gate, and multi-rule policy flows with design parity.
3. Policy create/edit/delete/list flows round-trip through the real API.
4. Validation and rejection paths match existing policy test steps (`20d`, `20e`).
5. Small backend/API/model adjustments are allowed only when required to support the UI parity and round-trip behavior above.

## Architectural Constraints
- UI views must remain presentation/composition focused; business logic should live in policy/domain/helper modules, not directly in large view functions.
- Reuse existing policy API models and DTO patterns where possible.
- Keep backend/API changes minimal and directly tied to UI round-trip correctness.
- If SQLx query shapes or migrations are changed, SQLx metadata must be refreshed using the repository devshell workflow.
- New UI behavior must be covered by the existing `web-ui` check flow, including screenshots/assertions for the Policies surface.

## Impact Areas
- `packages/web-ui/src/views/policies.rs`
- `packages/web-ui/src/components/policy/**`
- `checks/web-ui/tests/integration-test.js`
- Potentially deployment policy API/client/server model files if needed for parity round-trip behavior.
- Potentially SQLx metadata if backend query shapes change.

## Risk Level
Medium-high. The task touches a UI surface with modal workflows and may require constrained backend/API fixes for complete CRUD parity. Main risks are scope expansion, wasm build failures, and test/check flakiness in the web-ui integration check.

## Dependencies
- Parent umbrella: TASK-340.
- Design reference: `/home/mcamp/code/crystal-forge/CrystalForgelatest/components/PoliciesView.jsx`.
- Parity workflow: `design/doc-14 - Parity-execution-playbook-agent-proof.md`.
- Existing backend policy CRUD behavior from completed TASK-123/TASK-123.x and multi-rule/CVE policy behavior from TASK-176.

## Verification Plan
- `nix develop -c cargo fmt -- --check`
- `nix develop -c cargo check --manifest-path packages/web-ui/Cargo.toml --target wasm32-unknown-unknown`
- If backend/API code changes: targeted backend compile/tests appropriate to touched crate(s).
- If SQLx query shapes or migrations change: start local dev DB with process-compose and run `cargo sqlx prepare` or repository sqlx helper per SQLX SYNC REQUIREMENT.
- `nix build .#checks.x86_64-linux.web-ui`
- Confirm the `web-ui` check exercises/captures the relevant policy steps: `18-policies`, `19-policies-new-modal-basic`, `20-policies-new-modal-advanced`, `20b`, `20c`, `20d`, `20e`.

## Files
- `packages/web-ui/src/views/policies.rs`
- `packages/web-ui/src/components/policy/**`
- `checks/web-ui/tests/integration-test.js`
- Backend/API/model files only if directly required for policy CRUD round-trip parity.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Policies list layout, chips, and rule summaries match the design example
- [ ] #2 New/edit policy modal supports basic, advanced, CVE-gate, and multi-rule flows with design parity
- [ ] #3 Policy list/create/edit/delete flows round-trip through the real API
- [ ] #4 Validation/rejection paths behave per existing 20d/20e policy steps
- [ ] #5 web-ui policy steps pass with parity assertions and capture the Policies UI
- [ ] #6 Any backend/API/model changes are minimal and directly required for UI round-trip parity
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Update the Policies view to match the design example structure: page title/subtitle, category stat strip, filter bar with search/category/type filters, clear action, grouped policy sections, empty state, and API-backed counts where available.
2. Update policy display helpers/cards to render design-like policy cards with category rails, built-in/custom/protected/CVE chips, human-readable rule summaries, usage placeholders where real usage data is unavailable, and edit/delete controls only for editable policies.
3. Preserve existing real API-backed policy loading and CRUD behavior. Add helper mapping from backend policy payload/config to UI categories and rule summaries without moving business logic into the view.
4. Update the policy editor modal toward the design: New custom policy/edit wording, clearer metadata and rule-focused basic workflow, preserve advanced JSON/TOML editing, and add or improve multi-rule builder support if required for parity with basic/advanced/CVE/multi-rule flows.
5. Touch backend/API/model code only if the UI cannot round-trip supported policy shapes through existing APIs. If SQLx query shapes or migrations change, run the mandated devshell database/sqlx prepare workflow.
6. Extend checks/web-ui policy steps so they assert and screenshot the category/filter/card surface and modal workflows for basic, advanced, CVE-gate, multi-rule, and rejection paths.
7. Verify with nix develop -c cargo fmt -- --check, nix develop -c cargo check --manifest-path packages/web-ui/Cargo.toml --target wasm32-unknown-unknown, targeted backend checks only if backend code changes, SQLx prepare only if SQLx-affecting changes occur, and nix build .#checks.x86_64-linux.web-ui.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Approved implementation plan recorded before coding. Scope: Policies list/category/filter/card parity, policy editor modal parity including basic/advanced/CVE/multi-rule flows, web-ui parity assertions/screenshots, and only minimal backend/API/model changes if directly required for UI CRUD round-trip parity.
<!-- SECTION:NOTES:END -->
