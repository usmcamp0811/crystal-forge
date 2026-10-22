---
id: TASK-490.19
title: >-
  Assurance: STIG XCCDF import, preview atomicity, and framework mapping catalog
  scenario
status: In Progress
assignee:
  - '@openai'
created_date: '2026-10-17 16:38'
updated_date: '2026-10-22 22:20'
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
  - checks/integration-assurance/scenarios/fragments/stig-import.py
  - checks/integration-assurance/scenarios/fragments/stig-import.cjs
  - checks/integration-assurance/fixtures/stig-import-*
parent_task_id: TASK-490
priority: high
type: task
ordinal: 531000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Owning check: integration-assurance. Contract ID: assurance.policy.stig-import. Goal: prove input-derived import identities, preview atomicity, version provenance and framework mapping persistence. Risk: medium. Non-goals: enforcement (.18), export/round-trip assertions (.22), evaluation evidence, product changes and visual baselines without approved ImportStigModal state.

Writable paths (only):
- checks/integration-assurance/scenarios/fragments/stig-import.py
- checks/integration-assurance/scenarios/fragments/stig-import.cjs
- checks/integration-assurance/fixtures/stig-import-*

Composition target: checks/integration-assurance/scenarios/evidence-remediation-lifecycle.nix. Coordinator owns Nix/Python composition, browserSpecs mapping and explicit registration/normal owner aggregation. Import cases need only the controller/browser interface but compose inside the shared Assurance VM/database. No standalone import gate or packet-specific NixOS derivation. Later same-scenario policy/remediation/export cases MAY consume real imported identities through receipts. Another derivation's live rows, outputs or receipts MUST NOT be consumed.

Adapt V1R1/V1R2 owner-local fixtures byte-faithfully from stig-import-U_Anduril_NixOS_STIG_V1R1_Manual-xccdf.xml and V1R2 counterpart (Benchmark Anduril_NixOS_STIG). Preserve provenance; legacy input retirement remains coordinator-owned. Independently compute namespace-aware XML rule count, rule IDs/stable distinct subset and version into authored facts (for example build-time XML parsing); never derive expectations from CF responses. Author truncated XML and non-XCCDF inputs locally.

Preview V1R1 shows exact independently computed counts/identities and persists no policies/framework requirements. Valid V1R1 import persists exact rules/provenance; distinguishable V1R2 does not rewrite V1R1. Malformed/non-XCCDF inputs are visibly rejected without partial rows. UI mapping of an imported rule to a framework requirement persists through reload. Cover success, validation and preview loading only under approved state authority.

Authority/read-only sources: docs/knowledge/compliance/cf-xccdf-interchange-operator-guide.md, cf-xccdf-interchange-profile.md and stig-modules.md are as-built evidence; the XCCDF design review draft is not acceptance authority. Approved ImportStigModal.jsx, PoliciesView.jsx and PolicyEditor.jsx control presentation. Read cf-server api/compliance.rs xccdf_preview/xccdf_import/MAX_XCCDF_MULTIPART_BYTES, compliance/xccdf/zip_extractor.rs PackageKind and disa_stig_adapter.rs, api/framework_requirements.rs framework/version/search/mapping handlers, /api/v1/compliance/xccdf/{preview,import}, frameworks* and /api/v1/policy-versions/:pv/requirement-mappings; web-ui import/Provenance/Mappings tabs. Uncertain semantics remain blocked: decision/design gap. Production, helpers, composition, registration, CI, docs and design are read-only.

Dependency/readiness: TASK-490.1 controller foundation bba718befbc183863781656df2df272e5f735d5a on origin/dev 5ae7feab passed with .25/.26. Use concrete case/assert/receipt/sql_json/http named-session/CSRF and independent run_browser contexts. Await selection, fixture facts, accepted import/mapping authority and coordinator Assurance composition; import proofs themselves do not need future target roles. No import fragment execution is claimed.

Focused verification after composition: `nix build .#checks.x86_64-linux.integration-assurance.scenarios.evidence-remediation-lifecycle -L`. Coordinator records exact tested revision, command, result and evidence. Retain correct failures and linked defects; escalate interface/authority gaps.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Expected rule counts, identities, stable distinct subset and versions are independently derived from byte-faithful owner-local V1R1/V1R2 XCCDF inputs, never CF responses
- [ ] #2 Preview shows exact input-derived facts and persists no policy/framework rows; valid import persists exact rules/provenance and V1R2 does not rewrite V1R1
- [ ] #3 Malformed truncated XML and non-XCCDF inputs are visibly rejected without partial rows
- [ ] #4 An approved requirement mapping created through UI persists through authoritative database read and reload
- [ ] #5 Coordinator composes named import fragments into evidence-remediation-lifecycle with same-run identity receipts and no standalone import gate, packet derivation or cross-derivation live state
- [ ] #6 Coordinator executes the focused evidence-remediation-lifecycle gate and records exact tested revision, command, outcome and evidence; uncertain authority/design states remain explicitly blocked
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Preserve byte-faithful existing V1R1/V1R2 inputs and derive independent namespace-aware rule/version/identity facts from XML, plus authored malformed inputs.
2. Implement only named stig-import.py/.cjs and owner-local fixtures: actual preview with no policy/framework writes, real import/version preservation, visible malformed-input rejection and real UI requirement mapping with SQL/reload where approved.
3. Consume same-scenario controller/Admin helpers; expose typed facts/receipts and browser-phase interfaces to the coordinator. No new scenario/check, seeded outcome, mocked CF success, product fix, shared helper or registration change.
4. Verify syntax/input facts in the dedicated worker; coordinator composes into the existing Assurance lifecycle and executes its normal owner. Preserve any functional failure and escalate new defects rather than weakening accepted criteria.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Current readiness: controller prerequisite bba718be passed. Await selection, fixture-derived facts, accepted import/mapping states and coordinator Assurance composition. No import fragment run is claimed.

### Historical
2026-10-17 review recorded foundation/package blockers and no VM execution at that time. Legacy canonical STIG/20ad/20aa proofs were buried in the browser monolith and reconciliation used route mocks. A standalone import scenario was a former proposal; current import cases compose in the single Assurance lifecycle and retain owner-local input provenance.

Owner's wrap-up explicitly selects dependency-ready TASK-490 functional work; this controller-only contribution is selected, not blocked by D2 or target-mode coverage. Existing approved preview/import atomicity, input-derived identity and mapping criteria remain unchanged. Use the existing Assurance lifecycle; coordinator retains composition/normal-owner execution and all production files remain read-only for this test contribution.

Preflight: clean dedicated worktree/branch TASK-490.19-stig-import at0666c65e7e4bd424568f0800cdb5781c4d665556, based on current MR integration. Task-owned lock created. Coordinator retains main worktree, preview, composition and all VM ownership. No main integration worktree registered; existing dev untracked operational files and all other ownership remain preserved. Worker may not start a competing stack or VM.
<!-- SECTION:NOTES:END -->
