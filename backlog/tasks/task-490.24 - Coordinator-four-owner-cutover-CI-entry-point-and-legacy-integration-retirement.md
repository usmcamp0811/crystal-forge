---
id: TASK-490.24
title: >-
  Coordinator: four-owner cutover, CI entry point, and legacy integration
  retirement
status: Backlog
assignee: []
created_date: '2026-10-17 16:39'
updated_date: '2026-10-17 17:29'
labels:
  - testing
  - integration
  - coordinator
dependencies:
  - TASK-490.1
  - TASK-490.2
  - TASK-490.3
  - TASK-490.4
  - TASK-490.5
  - TASK-490.6
  - TASK-490.8
  - TASK-490.9
  - TASK-490.10
  - TASK-490.11
  - TASK-490.12
  - TASK-490.13
  - TASK-490.14
  - TASK-490.15
  - TASK-490.16
  - TASK-490.17
  - TASK-490.18
  - TASK-490.19
  - TASK-490.20
  - TASK-490.21
  - TASK-490.22
  - TASK-490.23
  - TASK-490.7
  - TASK-490.25
  - TASK-490.26
  - TASK-490.27
  - TASK-490.28
  - TASK-490.29
parent_task_id: TASK-490
priority: high
type: task
ordinal: 536000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Coordinator writer task, not a Luna packet. Owns checks/integration-{access,delivery,assurance,upgrade}/default.nix, shared lib/integration-test integration, .gitlab-ci.yml existing integration entry point, legacy check retirement, coverage map, contract status and directly affected testing/agent docs. Children supply disjoint scenario files and owner-local fixtures/assets; coordinator explicitly registers every integrated child.

Why: final inventory is exactly four native integration owners. Legacy checks/CI stay until passing replacements or explicit owner-approved scope change. This final cutover requirement does not prohibit discoverable meaningful WIP during foundation/child work.

Corrected architecture: owner default.nix uses native Snowfall signature { lib, inputs, pkgs, ... }, explicit scenario attrsets (Access coordinator-owned), and normal pkgs.linkFarm aggregate of ALL registered gates. Shared entry: lib.crystal-forge.makeIntegrationTestHarness { pkgs, inputs } from lib/integration-test. Scripts/fixtures/assets stay within each owning checks/integration-<owner>/ directory. No root tests/, legacyPackages.integration, mkOwner or custom discovery; no complete=true export switch. Only Access exists now. Add Delivery/Assurance/Upgrade default.nix only with meaningful scenarios, never placeholders; do not defer all owner creation to final completion.

Scope: integrate and explicitly register required child scenarios, ensuring every registered gate is a dependency of normal owner build and every failure propagates. Verify nix eval .#checks.x86_64-linux --apply builtins.attrNames identifies exactly the four integration names at final cutover plus classified non-integration checks. Put four-name allowlist comparison at start of existing integration CI entry point, no new validation service. Execute each complete required owner set and record normal runtime/peak resources. Retire legacy registration/job only after passing mapped replacement or explicit owner-approved scope change. Canonical scenario fixtures including STIG XML live under checks/integration-assurance/fixtures/; coordinate legacy fixture preservation/removal without cross-child writes. Update docs/knowledge/testing/flake-checks.md, web-ui-check.md, coverage map, contract implementation_status and directly affected verification/agent links. Final cutover blocked by any required failing replacement, including product defects, unless owner changes scope. No product fixes; TASK-490 test-only restrictions apply.

Candidates, NOT verified: nix build .#checks.x86_64-linux.integration-access -L, integration-delivery, integration-assurance and integration-upgrade through the same checks.x86_64-linux namespace. Focused foundation target: nix build .#checks.x86_64-linux.integration-access.scenarios.lifecycle-local-bootstrap -L, append .run for evidence. Future packet must provide exact verified commands at actual dependency commits. Record all owner outcomes/resources/evidence; evaluation/build alone does not establish real VM execution.

Authority: D1/as-built docs are implementation evidence, not accepted authority. Uncertain obligations remain blocked pending explicit owner decision. Upgrade D2 must be resolved before its replacement can pass.

READINESS: all child packets remain Backlog, none ready. Product blockers TASK-490.25/.26 were reproduced by Access VM at combined commit `fc930335878279ac426510359c039dfd8961dc13`; the startup assertion fails before migrations. Assurance packets .20/.21 were narrowed and .27/.28/.29 separate triage, waiver, acceptance-register and verification/closure. Child scenario runs cannot reuse live data or outputs from other scenarios. Product defects, accepted authority, complete owner registrations and passing full owner runs are required before cutover. No Luna delegation or executed owner is claimed.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 At final cutover exactly four native checks/integration-{access,delivery,assurance,upgrade}/default.nix owners use { lib, inputs, pkgs, ... }; each normal pkgs.linkFarm aggregate executes ALL registered gates and its complete required set, propagating every failure
- [ ] #2 CI runs the four owners through checks.x86_64-linux and checks the four-name allowlist in the existing entry point, without a new validation service
- [ ] #3 Every retired legacy check/job has a passing replacement recorded in the coverage map or explicit owner-approved scope change; failing product assertions remain executable and linked
- [ ] #4 Non-integration checks retain their own attributes and explicit classification
- [ ] #5 Runtime, peak resources, exact commands, revisions, real VM outcomes and .run evidence are recorded per owner
- [ ] #6 Coordinator explicitly registers child scenarios and maintains shared lib/CI/docs; scripts/fixtures/assets are owner-local, no root tests/, legacyPackages.integration, mkOwner, custom discovery, complete=true export switch or placeholder owners; meaningful WIP discovery is permitted before cutover
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-10-17 organization correction replaces old registration/layout/commands and incomplete-unexported rule. Coordinator writer, not Luna. Only Access exists now; remaining native owner default.nix files require meaningful scenarios, no placeholders. Foundation dependency commit unresolved; foundation blocked by two existing Rust package tests; no VM execution. All children Backlog, none ready. Future packets need actual dependency commit(s) and verified commands before To Do. TASK-490.20/.21 finer splitting remains a dispatch blocker without scope expansion now. D1/as-built is not authority; uncertain cases await owner decision. No delegation or executed slice is claimed.

Foundation execution update: TASK-491 dependency `af39284bdf16d56402f01a64c916f17948e43b02` was integrated as `a6c8922650bf7f0ff643d181422c0092cfc43242`. Focused Access VM command ran and failed on documented module configuration defects TASK-490.25/.26; exact result and redacted generated config are in docs/knowledge/testing/integration-coverage-map.md. Do not mark any child ready or retire legacy CI. No app fix/merge/deploy.
<!-- SECTION:NOTES:END -->
