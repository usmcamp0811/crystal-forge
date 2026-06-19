---
id: TASK-340.1
title: 'Policies: view + policy editor modal parity'
status: Review
assignee:
  - gpt-5.5
created_date: '2026-06-10 13:34'
updated_date: '2026-06-19 14:12'
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
  - TASK-340.2
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/281'
documentation:
  - design/doc-8 - CrystalForgelatest-UI-Parity-Matrix-TASK-328.md
  - design/doc-14 - Parity-execution-playbook-agent-proof.md
modified_files:
  - packages/web-ui/src/views/policies.rs
  - packages/web-ui/src/components/policy/mod.rs
  - packages/web-ui/src/components/policy/policy_card.rs
  - packages/web-ui/src/components/policy/policy_editor_modal.rs
  - packages/web-ui/src/components/policy/types.rs
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
- [x] #1 Policies list layout, chips, and rule summaries match the design example
- [x] #2 New/edit policy modal supports basic, advanced, CVE-gate, and multi-rule flows with design parity
- [x] #3 Policy list/create/edit/delete flows round-trip through the real API
- [x] #4 Validation/rejection paths behave per existing 20d/20e policy steps
- [x] #5 web-ui policy steps pass with parity assertions and capture the Policies UI
- [x] #6 Any backend/API/model changes are minimal and directly required for UI round-trip parity
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Force-pushed rebased branch to MR 281 with `git push --force-with-lease` (`41103a89...374e6ee4`).
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Force-pushed rebased MR branch with `git push --force-with-lease`: `41103a89...374e6ee4`.
<!-- SECTION:FINAL_SUMMARY:END -->
