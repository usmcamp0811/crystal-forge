---
id: TASK-490.22
title: 'Assurance: export downloads and policy/XCCDF interchange round-trip scenario'
status: Backlog
assignee: []
created_date: '2026-10-17 16:39'
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
Owning check: integration-assurance. Contract ID: assurance.interchange. Exclusive write paths: tests/integration/assurance/scenarios/interchange-exports.{nix,py,cjs}, tests/integration/assurance/fixtures/interchange-*. Separate from remediation because it needs independent parsers and schema validators, not workflow state.

WHY: legacy OSCAL/SARIF browser phases route-mock the compliance bundle API; oscal-export and xccdf-schema validate fixture output only. Actual downloads from real records have no proof.

SCOPE: using records produced by TASK-490.19/490.21 fixtures, download through the UI and parse independently: register export csv, xlsx, oscal-json, oscal-xml (OSCAL validated against pkgs.crystal-forge.oscal-1-1-2-schemas); POA&M and acceptance csv/xlsx; CVE csv; bundle-version and assignment XCCDF (validated with xmllint against vendored XCCDF schemas and schemas/cf-xccdf-1); policy interchange export then import into a fresh scope (preview persists nothing; import is atomic; identities round-trip); client-side SARIF and OSCAL downloads contain the expected records and scope. Expected record sets come from authored fixture facts, not from CF list responses. Non-goals: schema unit coverage already in oscal-export/xccdf-schema (non-integration, kept).

READY WHEN: TASK-490.19 and TASK-490.21 are integrated. Source to read then: cf-server handlers/api/register_export.rs, poam.rs (export), acceptance_register.rs (export), cves.rs (export), compliance.rs (export_bundle_xccdf, export_assignment_xccdf, policy_interchange_*); packages/web-ui/src/export/mod.rs; docs/knowledge/compliance/cf-xccdf-interchange-*.md; design AtoPackage.jsx.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Every supported server and client export is downloaded through the UI and parsed independently
- [ ] #2 OSCAL and XCCDF downloads validate against vendored schemas
- [ ] #3 Exported records and scope equal authored fixture facts
- [ ] #4 Policy interchange import is atomic and round-trips identities
- [ ] #5 Executed in real VMs with recorded evidence
<!-- AC:END -->
