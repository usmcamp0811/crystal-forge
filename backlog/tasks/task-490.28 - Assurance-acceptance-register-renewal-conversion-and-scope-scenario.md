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
Owning check: integration-assurance. Contract ID: assurance.remediation.acceptance-register. Exclusive paths: checks/integration-assurance/scenarios/remediation-acceptance-register.{nix,py,cjs}.

WHY: renew/convert in the accepted-register API are a distinct projection and actor flow from finding triage and from a policy finding-waiver request. They were previously bundled into TASK-490.20 without an authority/starting-state boundary.

OBJECTIVE: using actual source-owned acceptances created by the owner-authorized workflow (TASK-490.27 policy waiver or TASK-490.21 real CVE-derived plan), verify the acceptance register lists the exact source/identity/scope and that authorized renew and convert actions affect only that acceptance. Do not seed acceptance records. If the acceptance source or allowed transition is not specified by an approved requirement, mark that case blocked: decision instead of inferring from current handler behavior.

ACTOR/START/ACTION/RESULT: authorized Admin/Operator starts with one actual Accepted source record and one different source record. List/filter in UI; renew the chosen record -> exact review/expiration fields persist; convert to POA&M -> the correct source ID and evidence link transfer, with no duplicate or mutation of the other record. Unauthorized actor is rejected with no row change. Browser navigation returns to the originating finding/plan and labels the source accurately. Expected IDs/fields are authored or taken from producer receipts, never from the list response under test.

AUTHORITY AND SOURCE READING: first read accepted owner specifications (including the exact status and source limitations); docs/knowledge/poam/* only as as-built evidence. Read cf-server src/handlers/api/acceptance_register.rs (`list`, `renew`, `convert`, `RenewAcceptanceRequest`, `ConvertAcceptanceRequest`), src/queries/acceptance_register.rs (`AcceptanceSource` and scope/query), src/services/poam.rs renewal/conversion transaction functions, route registration; packages/web-ui acceptance-register and PoAM views; approved design component if the exact screen/state is present, otherwise mark visual case blocked: design gap.

HARNESS/REGISTRATION: shared Snowfall helper `lib.crystal-forge.makeIntegrationTestHarness {pkgs,inputs}` and signatures in coverage map. Requires producer receipts from TASK-490.27/.21; no direct SQL inserts. Coordinator explicitly registers `remediation-acceptance-register` in checks/integration-assurance/default.nix. Shared lib, owner default.nix and CI are read-only.

WRITABLE: checks/integration-assurance/scenarios/remediation-acceptance-register.{nix,py,cjs}; optional fixtures only beneath checks/integration-assurance/fixtures/acceptance-register-*. Other code, docs/design, helper, and check registration are read-only.

DEPENDENCIES/READINESS: TASK-490.27 and TASK-490.21 plus actual tested TASK-490.1 foundation and actual producer commits. Present foundation VM scenario fails on product configuration defects TASK-490.25/.26; this packet remains Backlog. No unverified future hashes.

FOCUSED COMMAND after dependency integration (not yet verified): `nix build .#checks.x86_64-linux.integration-assurance.scenarios.remediation-acceptance-register -L`; `.run` for evidence only. Coordinator runs complete `nix build .#checks.x86_64-linux.integration-assurance -L`. Limit to one VM run at a time pending capacity evidence.

Completion: exact starting acceptance source, actor authority, expected scope, forbidden cross-record effects, and UI navigation are independently asserted; focused command and complete owner registration are verified; real VM run and artifacts are recorded. Correct product failures remain executable and are linked to Backlog defect tasks; authority/design ambiguity is escalated to TASK-490 coordinator, not guessed.
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
Isolation clarification: TASK-490.27/.21 can provide source semantics and code prerequisites only. Their runtime acceptance/waiver/POA&M rows are not shared. In this scenario, generate an actual source-owned acceptance in the same fresh VM through its supported producer flow before testing register list/renew/convert. Do not seed acceptance records. Remain Backlog until a tested foundation and accepted source/renew/convert authority exist.
<!-- SECTION:NOTES:END -->
