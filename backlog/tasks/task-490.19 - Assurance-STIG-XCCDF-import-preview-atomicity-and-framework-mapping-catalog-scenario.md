---
id: TASK-490.19
title: >-
  Assurance: STIG XCCDF import, preview atomicity, and framework mapping catalog
  scenario
status: Backlog
assignee: []
created_date: '2026-10-17 16:38'
updated_date: '2026-10-17 17:28'
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
parent_task_id: TASK-490
priority: high
type: task
ordinal: 531000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Current packet boundary (2026-10-18): this task owns only XCCDF-import Python/browser fragments and owner-local XML fixtures under `checks/integration-assurance/scenarios/fragments/` and `fixtures/`. The coordinator owns `evidence-remediation-lifecycle.nix`, composition and registration. No packet-specific NixOS derivation; later same-run cases may use imported policy/evidence identities via receipts.

Owning check: integration-assurance. Contract ID: assurance.policy.stig-import. Exclusive writes: checks/integration-assurance/scenarios/policy-stig-import.{nix,py,cjs} and checks/integration-assurance/fixtures/stig-import-*. Controller-only scenario. Legacy canonical STIG/20ad/20aa proofs are buried in the browser monolith; reconciliation uses route mocks. Validate import atomicity using input-derived identities, never CF-derived expected results.

Non-goals: enforcement (490.18), export/interchange round-trip (490.22), evaluation evidence, production changes, visual baselines without approved ImportStigModal state. TASK-490 test-only scope applies.

Organization: native checks/integration-assurance/default.nix uses { lib, inputs, pkgs, ... }, explicit coordinator-owned scenarios and normal pkgs.linkFarm aggregate of ALL registered gates. Helpers lib/integration-test via lib.crystal-forge.makeIntegrationTestHarness { pkgs, inputs }. All scripts/fixtures/assets are owner-local. No root tests/, legacyPackages.integration, mkOwner or custom discovery. Coordinator owns default.nix/shared lib/CI/docs; child integration requires explicit registration. Meaningful WIP may be discoverable; only Access exists now; coordinator adds Assurance default.nix with meaningful scenario, never placeholder.

Authority review: docs/knowledge/compliance/cf-xccdf-interchange-operator-guide.md, cf-xccdf-interchange-profile.md and stig-modules.md are as-built evidence, not accepted D1 authority. XCCDF design review draft is not acceptance authority. Approved presentation references: ImportStigModal.jsx, PoliciesView.jsx, PolicyEditor.jsx, read-only. Uncertain semantics stay blocked pending owner decision; no Authority: as-built header.

Read-only source: cf-server api/compliance.rs xccdf_preview/xccdf_import/MAX_XCCDF_MULTIPART_BYTES, compliance/xccdf/zip_extractor.rs PackageKind and disa_stig_adapter.rs, api/framework_requirements.rs framework/version/search/mapping handlers; /api/v1/compliance/xccdf/{preview,import}, frameworks*, /api/v1/policy-versions/:pv/requirement-mappings; web-ui import and Provenance/Mappings tabs.

Inputs: owner-local fixtures stig-import-U_Anduril_NixOS_STIG_V1R1_Manual-xccdf.xml and V1R2 counterpart (Benchmark Anduril_NixOS_STIG), adapted byte-faithfully from legacy input by the future packet; legacy source preservation/retirement remains coordinator-owned. Scenario references only its owner-local fixture paths. Independently compute rule count, rule ids/stable distinct subset and version from XML at Nix build time, e.g. libxml2 xmllint namespace-aware XPath in pkgs.runCommand producing facts JSON. Author malformed truncated XML and non-XCCDF fixture locally; never derive expected counts from CF.

Verify actual foundation interface first. Pattern checks/integration-access/scenarios/lifecycle-local-bootstrap; controller browser role, API register/login admin setup with named session and CSRF, browser UI import, coverage-map helpers and ctx.download only if concretely available.

Scenario: preview V1R1 shows exact independently computed counts/identities and persists no policies/framework requirements. Import V1R1 persists exact rules/provenance; V1R2 is distinguishable and does not rewrite V1R1. Malformed/non-XCCDF rejected visibly with no partial rows. UI mapping of imported rule to framework requirement persists through reload. Viewer enforcement is 490.2. Cover success, validation error and preview loading only when approved state exists.

Candidates, NOT verified: nix build .#checks.x86_64-linux.integration-assurance.scenarios.policy-stig-import -L; append .run for evidence; nix eval .#checks.x86_64-linux.integration-assurance.scenarios --apply builtins.attrNames; aggregate nix build .#checks.x86_64-linux.integration-assurance -L. Run one VM test at a time until measured capacity supports a higher limit. Completion requires explicit registration, real VM execution and revision/commands/outcomes/evidence notes. Preserve correct failing assertions with linked Backlog product bugs; shared/authority gaps escalate to coordinator.

READINESS: actual foundation commit unresolved; foundation blocked by two existing Rust package tests; no VM execution. Backlog, not ready. Actual dependency commit(s), implemented helpers/fixtures/registration and verified commands required before To Do. No delegation or executed slice is claimed.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 policy-stig-import and its scripts/fixtures/assets live only under checks/integration-assurance/ and the coordinator explicitly registers the scenario in the native owner pkgs.linkFarm aggregate
- [ ] #2 Expected rule counts and identities are independently computed from owner-local XCCDF fixtures, not CF output
- [ ] #3 Preview persists nothing; import persists exactly expected rules/provenance; second version does not rewrite first
- [ ] #4 Malformed and non-XCCDF inputs are visibly rejected with no partial rows
- [ ] #5 A requirement mapping created in the UI persists through reload
- [ ] #6 Scenario executes in a real VM with revision, commands, outcomes and .run evidence recorded; uncertain authority cases remain blocked pending owner decision
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-10-17 organization correction replaces old scenario/fixture paths, commands and D1 authority instructions. Scenario inputs are owner-local, not runtime references to legacy browser fixtures. Coordinator owns owner default.nix/shared lib/CI/docs and explicit registration. Foundation commit unresolved; two existing Rust package tests block foundation; no VM execution. Backlog, not ready; actual dependency commit(s) and verified commands required before To Do. No Luna delegation or executed slice is claimed.
<!-- SECTION:NOTES:END -->
