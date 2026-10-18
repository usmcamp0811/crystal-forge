---
id: TASK-490.19
title: >-
  Assurance: STIG XCCDF import, preview atomicity, and framework mapping catalog
  scenario
status: Backlog
assignee: []
created_date: '2026-10-17 16:38'
updated_date: '2026-10-18 02:38'
labels:
  - testing
  - integration
  - luna
dependencies:
  - TASK-490.1
references:
  - docs/knowledge/testing/integration-coverage-map.md
  - docs/knowledge/compliance/cf-xccdf-interchange-operator-guide.md
  - docs/design/CrystalForge/components/ImportStigModal.jsx
  - >-
    checks/integration-assurance/fixtures/stig-import-U_Anduril_NixOS_STIG_V1R1_Manual-xccdf.xml
  - >-
    checks/integration-assurance/fixtures/stig-import-U_Anduril_NixOS_STIG_V1R2_Manual-xccdf.xml
modified_files:
  - checks/integration-assurance/scenarios/fragments/task-490.19-stig-import.py
  - checks/integration-assurance/scenarios/fragments/task-490.19-stig-import.cjs
  - checks/integration-assurance/fixtures/stig-import-*
parent_task_id: TASK-490
priority: high
type: task
ordinal: 531000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Owning check: integration-assurance. Contract ID: assurance.policy.stig-import. This packet owns only checks/integration-assurance/scenarios/fragments/task-490.19-stig-import.py/.cjs and owner-local XCCDF/malformed-input fixtures. Coordinator owns evidence-remediation-lifecycle.nix, Python composition, browserSpecs mapping and Assurance registration. No separate test derivation.

Within the same fresh Assurance VM, independently derive rule identities/counts from the XCCDF fixture. Prove preview persists nothing, valid imports persist exact identities/provenance, second versions do not rewrite the first, malformed and non-XCCDF inputs are rejected atomically, and approved framework mappings survive reload. Later policy/remediation/export fragments can use real imported/policy records and receipts. Never derive expected facts from CF responses or use another VM's rows.

As-built docs and design review drafts are not authority; uncertain outcomes remain blocked: decision/design gap. Writable paths are the named fragments and fixture files only. Production, shared helper, scenario composition, registration, CI and docs are read-only. Foundation and TASK-490.25/.26 must pass before dispatch; coordinator records the single Assurance VM result/evidence.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Expected rule counts and identities are independently computed from owner-local XCCDF fixtures
- [ ] #2 Preview persists nothing; valid import persists exact rules/provenance; later version does not rewrite prior version
- [ ] #3 Malformed and non-XCCDF inputs are rejected with no partial rows
- [ ] #4 Approved requirement mapping persists through reload
- [ ] #5 The coordinator composes this fragment into evidence-remediation-lifecycle; no packet derivation is added
- [ ] #6 The coordinator records real Assurance VM revision, command, outcome and evidence
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-10-17 organization correction replaces old scenario/fixture paths, commands and D1 authority instructions. Scenario inputs are owner-local, not runtime references to legacy browser fixtures. Coordinator owns owner default.nix/shared lib/CI/docs and explicit registration. Foundation commit unresolved; two existing Rust package tests block foundation; no VM execution. Backlog, not ready; actual dependency commit(s) and verified commands required before To Do. No Luna delegation or executed slice is claimed.
<!-- SECTION:NOTES:END -->
