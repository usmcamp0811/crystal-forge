---
id: TASK-340.1
title: 'Policies: view + policy editor modal parity'
status: Backlog
assignee: []
created_date: '2026-06-10 13:34'
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

## Problem
The Policies view (`views/policies.rs`, route `/deployment-policies`) must match `CrystalForgelatest/components/PoliciesView.jsx`, including the multi-rule/CVE-gate policy editor.

## Goal
Bring the Policies list + policy editor modal to parity, backed by real API CRUD.

## Exact scope
1. Policies list layout, chips, and rule summaries match design.
2. New/edit policy modal (basic + advanced + CVE-gate + multi-rule) matches design and round-trips via real API.
3. Validation/rejection paths match existing steps (`20d`, `20e`).

## Non-goals
- Compliance view (separate surface TASK-344).

## Files
- packages/web-ui/src/views/policies.rs
- packages/web-ui/src/components/policy/**
- checks/web-ui/tests/integration-test.js

## Verification
- nix develop -c cargo fmt -- --check
- nix develop -c cargo check --manifest-path packages/web-ui/Cargo.toml --target wasm32-unknown-unknown
- nix build .#checks.x86_64-linux.web-ui
- Reuse steps `18-policies`, `19-policies-new-modal-basic`, `20-policies-new-modal-advanced`, `20b/20c/20d/20e`.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Policies list layout, chips, and rule summaries match the design
- [ ] #2 New/edit policy modal (basic/advanced/CVE-gate/multi-rule) matches design and round-trips via real API
- [ ] #3 Validation/rejection paths behave per the existing 20d/20e steps
- [ ] #4 web-ui policy steps pass with parity assertions
<!-- AC:END -->
