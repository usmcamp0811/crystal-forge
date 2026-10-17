---
id: TASK-490.22
title: 'Assurance: export downloads and policy/XCCDF interchange round-trip scenario'
status: Backlog
assignee: []
created_date: '2026-10-17 16:39'
updated_date: '2026-10-17 17:28'
labels:
  - testing
  - integration
  - luna
dependencies:
  - TASK-490.21
  - TASK-490.19
parent_task_id: TASK-490
priority: medium
type: task
ordinal: 534000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Owning check: integration-assurance. Contract ID: assurance.interchange. Exclusive writes: checks/integration-assurance/scenarios/interchange-exports.{nix,py,cjs} and checks/integration-assurance/fixtures/interchange-*. Separate from remediation: independent parsers/schema validators. Legacy OSCAL/SARIF phases mock bundle API; oscal-export/xccdf-schema validate fixture output, not real-record downloads.

Scope: reuse records produced by TASK-490.19/.21 fixtures. UI-download and independently parse register csv/xlsx/oscal-json/oscal-xml; validate OSCAL with pkgs.crystal-forge.oscal-1-1-2-schemas. Parse POA&M/acceptance csv/xlsx and CVE csv; bundle-version/assignment XCCDF validates with xmllint against vendored XCCDF and schemas/cf-xccdf-1. Policy interchange export/import into fresh scope: preview persists nothing, import atomic, identities round-trip. Client-side SARIF/OSCAL contains expected records/scope. Expected sets derive from authored fixture facts, never CF lists. Scenario-specific schema/data assets remain owner-local or consume existing immutable package inputs. Non-goals: schema-unit coverage in retained non-integration oscal-export/xccdf-schema. TASK-490 test-only restrictions apply.

Organization: native checks/integration-assurance/default.nix uses { lib, inputs, pkgs, ... }, coordinator-owned explicit scenarios and normal pkgs.linkFarm aggregate of ALL registered gates. Shared helpers lib/integration-test via lib.crystal-forge.makeIntegrationTestHarness { pkgs, inputs }; scripts/fixtures/assets owner-local. No root tests/, legacyPackages.integration, mkOwner or custom discovery. Coordinator owns default.nix/shared lib/CI/docs; child integration requires explicit registration and disjoint files. Meaningful WIP may be discoverable; only Access exists now, other owners need meaningful scenarios, no placeholders.

Prerequisites: TASK-490.19/.21 integrated, including resolution of their packet-readiness blockers. Read cf-server api/register_export.rs, poam.rs export, acceptance_register.rs export, cves.rs export, compliance.rs export_bundle_xccdf/export_assignment_xccdf/policy_interchange_*; web-ui src/export/mod.rs; docs/knowledge/compliance/cf-xccdf-interchange-*.md and AtoPackage.jsx. As-built D1 is evidence, not authority; uncertain supported formats/semantics blocked pending owner decision.

Candidate, NOT verified: nix build .#checks.x86_64-linux.integration-assurance.scenarios.interchange-exports -L; append .run for evidence; aggregate nix build .#checks.x86_64-linux.integration-assurance -L. Completion requires explicit registration, real VM execution and exact revision/commands/outcomes/evidence. Preserve correct failing assertions and linked Backlog product bugs; escalate helper/authority gaps.

READINESS: actual foundation commit unresolved; two existing Rust package tests block foundation; no VM execution. Backlog, not ready. Actual dependency commit(s), implemented interfaces/fixtures/registration and verified commands required before To Do. No delegation or executed slice is claimed.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Every supported server and client export is downloaded through the UI and parsed independently
- [ ] #2 OSCAL and XCCDF downloads validate against vendored schemas
- [ ] #3 Exported records and scope equal authored fixture facts
- [ ] #4 Policy interchange import is atomic and round-trips identities
- [ ] #5 Executed in real VMs with recorded evidence
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-10-17 organization correction replaces old layout/commands with native owner-local paths and explicit coordinator registration. Foundation dependency commit unresolved; foundation blocked by two existing Rust package tests; no VM execution. Backlog, not ready. Future packet needs actual dependency commit(s) and verified commands before To Do; inherited .21 refinement blocker also remains. No Luna delegation or executed slice is claimed.
<!-- SECTION:NOTES:END -->
