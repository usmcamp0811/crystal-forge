---
id: TASK-490.22
title: 'Assurance: export downloads and policy/XCCDF interchange round-trip scenario'
status: Backlog
assignee: []
created_date: '2026-10-17 16:39'
updated_date: '2026-10-17 20:33'
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
Owning check: integration-assurance. Contract ID: assurance.interchange. Exclusive writes: checks/integration-assurance/scenarios/interchange-exports.{nix,py,cjs}, owner-local interchange fixtures only. This packet is separate from remediation because it needs independent parsers/schema validators and real authenticated browser downloads.

WHY: legacy OSCAL/SARIF browser phases route-mock bundle data; oscal-export and xccdf-schema validate fixture/writer outputs, not actual downloads from CF records.

SCOPE: create the small required source records through real UI/API setup within this scenario's own fresh VM (never import rows or outputs from TASK-490.19/.21/.27/.28 runs). Download via the production UI and parse independently: register CSV/XLSX/OSCAL JSON/XML; POA&M CSV/XLSX; acceptance CSV/XLSX; CVE CSV; bundle-version/assignment XCCDF; policy interchange export, preview and atomic import into a new scope; client-side SARIF/OSCAL downloads. Expected record IDs/scope come from authored prerequisite facts and receipts from setup actions, not from the response under test. Validate OSCAL/XCCDF with vendored schemas. Non-goals: repeat full POA&M/CVE/policy workflows, schema-unit coverage, production changes.

DEPENDENCY: TASK-490.19 for accepted supported STIG/import format context and TASK-490.21/.28 for source record semantics only; their live data is NOT shared. Requires the tested TASK-490.1 foundation and integrated helper dependencies. Current Access VM run fails on product defects TASK-490.25/.26; remain Backlog.

Read-only sources: cf-server src/handlers/api/register_export.rs, poam.rs, acceptance_register.rs, cves.rs, compliance.rs (exports and policy_interchange_*); packages/web-ui/src/export/mod.rs and export UI; docs/knowledge/compliance/cf-xccdf-interchange-*.md; approved AtoPackage.jsx only for presentation. Non-integration schema checks oscal-export/xccdf-schema stay intact.

Writable paths: checks/integration-assurance/scenarios/interchange-exports.{nix,py,cjs}, checks/integration-assurance/fixtures/interchange-* only. Owner default.nix registration, lib/integration-test, flake/CI and all production/design sources are read-only; coordinator explicitly registers it.

Harness: `lib.crystal-forge.makeIntegrationTestHarness {pkgs,inputs}`; each run creates its own DB/VM. Focused command `nix build .#checks.x86_64-linux.integration-assurance.scenarios.interchange-exports -L` is a candidate, NOT yet verified; `.run` for evidence only. Coordinator runs complete `nix build .#checks.x86_64-linux.integration-assurance -L`.

Completion requires independent parse/schema assertions, downloads from a real authenticated browser, exact expected record IDs/scope, actual complete-owner registration and execution. Preserve correct failure; link a Backlog product defect. No task readiness until exact integrated foundation commits and focused commands are verified.
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
