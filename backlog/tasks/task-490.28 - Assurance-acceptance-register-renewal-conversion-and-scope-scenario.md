---
id: TASK-490.28
title: 'Assurance: acceptance-register renewal, conversion, and scope scenario'
status: Backlog
assignee: []
created_date: '2026-10-17 20:31'
updated_date: '2026-10-17 20:33'
labels:
  - testing
  - integration
  - luna
dependencies:
  - TASK-490.27
  - TASK-490.21
parent_task_id: TASK-490
priority: medium
type: task
ordinal: 541000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Current packet boundary (2026-10-18): this task owns acceptance-register Python/browser fragments only. The coordinator owns the shared `evidence-remediation-lifecycle.nix`, composition and registration. Create the source acceptance earlier in that same fresh Assurance VM and pass generated IDs via receipts; no packet-specific derivation or runtime reuse from .27/.21.

Owning check: integration-assurance. Contract ID: assurance.remediation.acceptance-register. Exclusive paths: checks/integration-assurance/scenarios/remediation-acceptance-register.{nix,py,cjs}.

WHY: renew/convert in the accepted-register API are a distinct projection and actor flow from finding triage and from a policy finding-waiver request. They were previously bundled into TASK-490.20 without an authority/starting-state boundary.

OBJECTIVE: create a real source-owned acceptance within this scenario's own fresh VM through its supported producer workflow; verify register listing, exact source/identity/scope, and that authorized renew/convert actions affect only that acceptance. Do not seed acceptance records. If source or transition is not specified by approved requirements, mark the case blocked: decision.

ACTOR/START/ACTION/RESULT: authorized Admin/Operator starts with one actual Accepted source record and one different source record. List/filter in UI; renew the chosen record -> exact review/expiration fields persist; convert to POA&M -> exact source ID/evidence link transfer, with no duplicate or mutation of the other record. Unauthorized actor is rejected with no row change. Browser return navigation preserves the originating finding/plan identity. Expected IDs/fields are authored or setup-action receipts, never the list response under test.

AUTHORITY AND SOURCE READING: read accepted owner specifications and exact source limits; docs/knowledge/poam/* is as-built evidence only. Read cf-server src/handlers/api/acceptance_register.rs (`list`, `renew`, `convert`, request types), src/queries/acceptance_register.rs (`AcceptanceSource` and scope), src/services/poam.rs renew/convert transactions, router; UI acceptance-register and PoA&M views; approved design component if present, else visual case blocked: design gap.

HARNESS/REGISTRATION: shared `lib.crystal-forge.makeIntegrationTestHarness {pkgs,inputs}`; scenario receives `{harness, owner, name}` and uses documented `http`, `sql_json`, `run_browser`, case/assert/receipt helpers. Producer code/semantics from TASK-490.27/.21 may be prerequisites; their live acceptance/waiver/POA&M rows cannot be shared. Coordinator explicitly registers the scenario. Shared lib and owner default.nix are coordinator-owned.

WRITABLE: checks/integration-assurance/scenarios/remediation-acceptance-register.{nix,py,cjs}; optional fixtures beneath checks/integration-assurance/fixtures/acceptance-register-* only. All other production, design, helper, registration and CI paths are read-only.

DEPENDENCIES/READINESS: TASK-490.27/.21 and actual tested foundation and producer commits. Current foundation VM scenario fails on TASK-490.25/.26; remain Backlog.

FOCUSED COMMAND after dependency integration (not yet verified): `nix build .#checks.x86_64-linux.integration-assurance.scenarios.remediation-acceptance-register -L`; `.run` for evidence only. Coordinator runs `nix build .#checks.x86_64-linux.integration-assurance -L`. One VM at a time pending capacity measurement.

Completion: exact source, actor authority, scope and forbidden cross-record effects asserted; scenario explicitly registered and in full owner; real VM outcome, tested revision, command and evidence recorded. Keep correct failures and link defects; escalate authority/interface issues. Not ready until tested foundation and accepted source/transition authority exist.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The acceptance register begins with a real source-owned acceptance and no acceptance row is seeded
- [ ] #2 List/filter results and source identity match authored/producer facts
- [ ] #3 Renew changes only the selected acceptance and persists the approved fields
- [ ] #4 Convert creates a POA&M with the exact source/evidence identity and does not duplicate or alter another acceptance
- [ ] #5 Unauthorized actions fail without side effects and browser return navigation preserves source identity
- [ ] #6 Scenario is registered and executed in the complete Assurance owner with exact commands, revision, results, and evidence
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Isolation clarification: TASK-490.27/.21 are code/semantics dependencies only. Their runtime records cannot be reused; create an actual accepted source record inside this scenario's own fresh VM through the supported workflow. Remain Backlog pending tested foundation and accepted authority.
<!-- SECTION:NOTES:END -->
