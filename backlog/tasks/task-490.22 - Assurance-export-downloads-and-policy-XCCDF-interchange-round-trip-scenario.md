---
id: TASK-490.22
title: 'Assurance: export downloads and policy/XCCDF interchange round-trip scenario'
status: Backlog
assignee: []
created_date: '2026-10-17 16:39'
updated_date: '2026-10-18 02:38'
labels:
  - testing
  - integration
  - luna
dependencies:
  - TASK-490.21
  - TASK-490.19
modified_files:
  - checks/integration-assurance/scenarios/fragments/task-490.22-exports.py
  - checks/integration-assurance/scenarios/fragments/task-490.22-exports.cjs
  - checks/integration-assurance/fixtures/interchange-*
parent_task_id: TASK-490
priority: medium
type: task
ordinal: 534000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Owning check: integration-assurance. Contract ID: assurance.interchange. This packet owns only checks/integration-assurance/scenarios/fragments/task-490.22-exports.py/.cjs and owner-local parser/schema fixtures. Coordinator owns the shared evidence-remediation-lifecycle.nix scenario, Python composition, browserSpecs mapping and Assurance registration; no packet-specific NixOS derivation.

After earlier real import, scan, acceptance and POA&M cases create source records in the same fresh Assurance VM, download the supported CSV/XLSX/OSCAL/XCCDF/policy formats through the production UI and parse them independently. Assert authored source IDs/scope via receipts, validate OSCAL/XCCDF against vendored schemas, and prove policy interchange import is atomic and round-trips identities into a new scope. Do not consume records or downloads from .19/.21/.27/.28 derivations and do not seed the exports under test.

Writable paths are the named fragments and owner-local fixtures. Production, shared helpers, scenario composition, registration, CI and docs are read-only. Requires the evidence-remediation producers and passing TASK-490.1/.25/.26. Coordinator records actual Assurance VM result/evidence.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Each supported export is downloaded from the production UI and parsed independently
- [ ] #2 OSCAL/XCCDF downloads validate against vendored schemas
- [ ] #3 Exported records and scope match authored facts and same-run receipts
- [ ] #4 Policy interchange import is atomic and round-trips identities
- [ ] #5 The coordinator composes this fragment after its source records are created in evidence-remediation-lifecycle; no packet derivation is added
- [ ] #6 The coordinator records the single Assurance VM tested revision, command, result and evidence
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-10-17 organization correction replaces old layout/commands with native owner-local paths and explicit coordinator registration. Foundation dependency commit unresolved; foundation blocked by two existing Rust package tests; no VM execution. Backlog, not ready. Future packet needs actual dependency commit(s) and verified commands before To Do; inherited .21 refinement blocker also remains. No Luna delegation or executed slice is claimed.
<!-- SECTION:NOTES:END -->
