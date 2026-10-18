---
id: TASK-490.22
title: 'Assurance: export downloads and policy/XCCDF interchange round-trip scenario'
status: Backlog
assignee: []
created_date: '2026-10-17 16:39'
updated_date: '2026-10-18 13:59'
labels:
  - testing
  - integration
  - luna
dependencies:
  - TASK-490.19
  - TASK-490.21
  - TASK-490.27
  - TASK-490.28
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
Owning check: integration-assurance. Contract ID: assurance.interchange. Goal: validate real-record UI downloads with independent parsers/schema validators and prove atomic policy interchange. Risk: medium. Non-goals: retained schema-unit coverage in oscal-export/xccdf-schema, production changes.

Writable paths (only):
- checks/integration-assurance/scenarios/fragments/task-490.22-exports.py
- checks/integration-assurance/scenarios/fragments/task-490.22-exports.cjs
- checks/integration-assurance/fixtures/interchange-*

Composition target: checks/integration-assurance/scenarios/evidence-remediation-lifecycle.nix. Coordinator owns Nix/Python composition, browserSpecs and explicit registration/normal owner aggregation. Compose after real import/scan/plan/waiver/acceptance producers in one fresh Assurance VM/database. Earlier real records from .19/.16/.21/.27/.28 MAY be consumed through receipts; another derivation's live records, downloads, outputs or receipts MUST NOT be consumed. No packet-specific derivation; never seed source records or export outcomes.

Through production UI download and independently parse register csv/xlsx/oscal-json/oscal-xml, POA&M and acceptance csv/xlsx, CVE csv, and supported client-side SARIF/OSCAL. Validate OSCAL against existing immutable oscal-1-1-2-schemas package input. Validate bundle-version and assignment XCCDF with an independent XML validator against vendored XCCDF and schemas/cf-xccdf-1 schemas. Expected record sets/scope come from authored facts and same-run producer receipts, never CF lists being tested. Schema/data fixtures remain owner-local or existing immutable package inputs.

Policy interchange export/import into a new scope: preview persists nothing, malformed/failed import has no partial rows, valid import is atomic and round-trips exact identities. Supported formats/source limits and success/error states must follow accepted authority; uncertain cases remain blocked: decision/design gap.

Dependencies/readiness: TASK-490.19 import, .21 plans, .27 waiver and .28 source-owned acceptance producers must be integrated with their real scan/policy setup and exact receipts at recorded tested revisions. .27/.28 are explicit dependencies because acceptance exports need their source records. Foundation bba718befbc183863781656df2df272e5f735d5a on origin/dev 5ae7feab passed with .25/.26; old startup/decomposition blockers are historical. Await selection, concrete download/parser/schema interfaces, accepted format/atomicity authority and coordinator ordering. No export fragment run is claimed.

Read-only sources: cf-server api/register_export.rs, poam.rs export, acceptance_register.rs export, cves.rs export, compliance.rs export_bundle_xccdf/export_assignment_xccdf/policy_interchange_*; web-ui src/export/mod.rs; docs/knowledge/compliance/cf-xccdf-interchange-*.md as as-built evidence; approved AtoPackage.jsx. Production, shared helpers, composition, registration, CI, docs and design are read-only.

Focused verification after composition: `nix build .#checks.x86_64-linux.integration-assurance.scenarios.evidence-remediation-lifecycle -L`. Coordinator records exact tested revision, command, result and evidence; preserve correct failures and linked defects and escalate helper/authority gaps.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Every supported server/client export, including register CSV/XLSX/OSCAL JSON/XML, POA&M/acceptance CSV/XLSX, CVE CSV and client SARIF/OSCAL, is downloaded through production UI and parsed independently
- [ ] #2 OSCAL and bundle-version/assignment XCCDF downloads validate against the stated immutable/vendored OSCAL, XCCDF and CF extension schemas
- [ ] #3 Exported exact records and scope match independently authored facts and earlier same-scenario producer receipts, never CF lists under test
- [ ] #4 Policy interchange preview persists nothing; malformed/failed imports have no partial rows and valid atomic import into a new scope round-trips identities
- [ ] #5 Coordinator composes named export fragments after .19/.16/.21/.27/.28 source records in evidence-remediation-lifecycle; no seeded result, packet derivation or cross-derivation live-state/download reuse occurs
- [ ] #6 Coordinator executes the focused evidence-remediation-lifecycle gate and records exact tested revision, command, result and evidence
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Current readiness: foundation bba718be passed. Real import/scan/plan/waiver/acceptance producers, accepted formats/atomicity, concrete download/parser/schema interfaces and coordinator ordering remain prerequisites. No export packet run is claimed.

### Historical
2026-10-17 review recorded unresolved foundation/package blockers and inherited plan decomposition work. That decomposition now exists in .21/.27/.28/.29. Legacy OSCAL/SARIF browser phases mocked bundle APIs; schema checks validated fixtures rather than these real downloads. Current export cases use earlier real same-scenario source records through receipts, not another derivation's state.
<!-- SECTION:NOTES:END -->
