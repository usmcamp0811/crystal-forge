---
id: TASK-490.19
title: >-
  Assurance: STIG XCCDF import, preview atomicity, and framework mapping catalog
  scenario
status: Backlog
assignee: []
created_date: '2026-10-17 16:38'
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
  - checks/web-ui/tests/fixtures/U_Anduril_NixOS_STIG_V1R1_Manual-xccdf.xml
parent_task_id: TASK-490
priority: high
type: task
ordinal: 531000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Owning check: integration-assurance. Contract ID: assurance.policy.stig-import (see docs/knowledge/testing/integration-coverage-map.md).

WHY: the strongest legacy proofs (task433-canonical-imported-stig-refinement, 20ad, 20aa) live inside the 26.9k-line web-ui suite with strict baselines; web-ui-reconciliation proves the import UI only against route mocks. Import must be validated and atomic, and its expected identities must come from the input file, not from CF.

OBJECTIVE: one scenario tests/integration/assurance/scenarios/policy-stig-import.{nix,py,cjs} that needs only the controller role.

NON-GOALS: enforcement and admission (TASK-490.18); exports and interchange round-trip (TASK-490.22); evidence from evaluations; production changes; visual baselines unless an approved ImportStigModal state applies.

BASE: foundation commit in implementation notes (FOUNDATION_COMMIT). Pattern: tests/integration/access/scenarios/lifecycle-local-bootstrap.{nix,py,cjs}.

AUTHORITY: docs/knowledge/compliance/cf-xccdf-interchange-operator-guide.md; docs/knowledge/compliance/cf-xccdf-interchange-profile.md; docs/knowledge/compliance/stig-modules.md. The XCCDF interchange design doc in docs/design is a review draft and is NOT acceptance authority. Approved presentation reference: docs/design/CrystalForge/components/ImportStigModal.jsx, PoliciesView.jsx, PolicyEditor.jsx. Header: `Authority: as-built (D1)`.

PRODUCTION SOURCE TO READ (read-only): cf-server src/handlers/api/compliance.rs (xccdf_preview ~4649, xccdf_import ~4961; multipart limit MAX_XCCDF_MULTIPART_BYTES), src/compliance/xccdf/zip_extractor.rs (PackageKind Xml|Zip), src/compliance/xccdf/disa_stig_adapter.rs, src/handlers/api/framework_requirements.rs (list_compliance_frameworks, list_compliance_framework_versions, search_framework_requirements, list/create/update_policy_requirement_mapping); routes /api/v1/compliance/xccdf/{preview,import}, /api/v1/compliance/frameworks*, /api/v1/policy-versions/:pv/requirement-mappings; packages/web-ui ImportStigModal and policy editor Provenance/Mappings tabs.

INPUT FIXTURES (read-only, legacy location; the coordinator moves them at cutover): checks/web-ui/tests/fixtures/U_Anduril_NixOS_STIG_V1R1_Manual-xccdf.xml and U_Anduril_NixOS_STIG_V1R2_Manual-xccdf.xml (Benchmark id Anduril_NixOS_STIG). Reference them from the scenario .nix with "${harness.inputs.self}/checks/web-ui/tests/fixtures/<file>". Compute the expected facts INDEPENDENTLY at Nix build time with an XML tool (e.g. a pkgs.runCommand using libxml2's xmllint --xpath over the XCCDF namespace) into a facts JSON: rule count, rule ids (or a stable subset with distinct identities), and the version string per file. Do not count from CF output. Exclusive write paths: tests/integration/assurance/scenarios/policy-stig-import.{nix,py,cjs} and tests/integration/assurance/fixtures/stig-import-* (for a deliberately malformed variant you author).

HARNESS: nodes.controller = harness.roles.controller { browser = true; }; admin setup via API (register + login with an http() session, csrf=True for mutations); browser sign-in in the spec for the UI import; helpers per the coverage map harness section; ctx.download is available if you need to save a preview artifact.

SCENARIO policy-stig-import (actor: admin):
1. Preview V1R1 through the Import STIG UI: preview shows the expected rule count/identities; DB rows for policies/framework requirements are unchanged after preview (atomic preview, nothing persisted).
2. Import V1R1: persisted rules equal the independently computed set (count and identities); provenance shows the source benchmark/version.
3. Import V1R2: the second version is distinguishable; V1R1 records are not rewritten.
4. Malformed input (authored truncated XML) and a non-XCCDF file are rejected with a visible error and no partial rows (counts before == after).
5. Create one requirement mapping from an imported rule to a framework requirement through the UI; it persists through reload.
6. Viewer is not required here (role enforcement is TASK-490.2).
UI states: success, validation error with no persistence, loading during preview if the design defines it.

COMMANDS: nix build .#integration.assurance.scenarios.policy-stig-import -L ; append .run for evidence after failure; nix eval .#integration.assurance.scenarios --apply builtins.attrNames must list it. At most two VM tests concurrently on this host. Coordinator runs .#integration.assurance.all.

COMPLETION, PRODUCT DEFECT, ESCALATION: same rules as TASK-490.2 (real VM execution; commands/revision/outcome/evidence in notes; correct failing assertions stay failing with a linked Backlog bug; interface or authority problems go to the coordinator, claude-opus-5-5; never edit coordinator-owned files).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 policy-stig-import exists only under tests/integration/assurance/scenarios/ and is discovered
- [ ] #2 Expected rule counts and identities are computed from the XCCDF files independently of CF
- [ ] #3 Preview persists nothing; import persists exactly the expected rules with provenance; a second version does not rewrite the first
- [ ] #4 Malformed and non-XCCDF inputs are rejected with no partial rows
- [ ] #5 A requirement mapping created in the UI persists through reload
- [ ] #6 Executed in a real VM with commands, revision, outcome, and evidence paths in task notes
<!-- AC:END -->
