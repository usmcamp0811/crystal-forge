---
id: TASK-490.27
title: 'Assurance: finding-waiver lifecycle and evidence separation scenario'
status: Backlog
assignee: []
created_date: '2026-10-17 20:31'
updated_date: '2026-10-17 20:33'
labels:
  - testing
  - integration
  - luna
dependencies:
  - TASK-490.16
parent_task_id: TASK-490
priority: high
type: task
ordinal: 540000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Owning check: integration-assurance. Contract ID: assurance.remediation.finding-waiver. Exclusive paths: checks/integration-assurance/scenarios/remediation-finding-waiver.{nix,py,cjs}; owner-local fixtures only if needed.

WHY: the old TASK-490.20 packet mixed triage, waiver, and acceptance-register flows with different starting states and authority. This packet isolates finding waivers. Triage is TASK-490.20; acceptances register is TASK-490.28.

OBJECTIVE: starting with a real finding emitted by TASK-490.16 (not seeded evidence), exercise only the supported finding-waiver lifecycle and prove that accepted/rejected/revoked/expired waiver decisions remain separate from remediation.

SCENARIO: actor/scope and exact UI entry point must follow an owner-approved source. Submit waiver request for one distinct finding; authorizer reviews Accepted or Rejected as applicable; expiration is tested with an authored due time or injected deterministic clock only if an approved API/test boundary supports it (do not add test clocks); revoke an accepted waiver. Independently read persistence and UI. Negative: a waiver never marks the finding remediated, never alters scan evidence or Current status, and unauthorized review has no side effect.

AUTHORITY / READ-ONLY SOURCES: first read docs/design/CrystalForge/docs/crystal-forge-systems-design/cve-poam-evidence-continuity-design-spec.md and determine whether it approves these exact waiver interactions; read docs/knowledge/cves/* and poam/* as as-built only. If approval or designed UI state is absent, record `blocked: decision` or `blocked: design gap`; do not infer behavior from `WaiverDecision` variants alone. Inspect packages/default/crates/cf-server/src/handlers/api/poam.rs finding-waivers routes/handlers; src/models/poam.rs `WaiverDecision`; router in src/bin/server.rs; UI waiver/modal caller and docs/design/CrystalForge/PoamViews.jsx/CveBatchTriage.jsx.

HARNESS: eventual native Snowfall signature `{lib, inputs, pkgs, ...}`; shared `lib.crystal-forge.makeIntegrationTestHarness {pkgs,inputs}`. Scenario receives `{harness, owner, name}`. Use real `controller` and real scan setup within this scenario's fresh VM; helpers `case`, `expect`, `expect_equal`, `receipt`, `http`, `sql_json`, `run_browser`. No other scenario's runtime findings/receipts are available; no fabricated findings or direct result inserts. Refer to docs/knowledge/testing/integration-coverage-map.md#harness-interface. Coordinator owns helper/default.nix and explicit registration.

WRITABLE (exclusive): checks/integration-assurance/scenarios/remediation-finding-waiver.{nix,py,cjs}, checks/integration-assurance/fixtures/finding-waiver-* only. READ-ONLY: all other paths, production code, docs/design/CrystalForge, lib/integration-test, owner default.nix, flake and CI.

DEPENDENCIES: TASK-490.16 and the tested TASK-490.1 foundation commit must be integrated. The current foundation Access slice fails on product defects TASK-490.25/.26; no tested ready foundation commit exists. Remain Backlog.

FOCUSED COMMAND (not yet verified): `nix build .#checks.x86_64-linux.integration-assurance.scenarios.remediation-finding-waiver -L`; `.run` for evidence only. Coordinator runs `nix build .#checks.x86_64-linux.integration-assurance -L`. One VM test at a time until capacity is measured.

Completion: approved actor/authority/UI expectations recorded; scenario explicitly registered; real VM run executed with no seeded result; independent finding identity, side-effect negatives and evidence paths recorded. If a correct assertion fails, retain it and create/link a narrow Backlog product defect. Shared interface or authority conflicts go to the TASK-490 coordinator. Remain Backlog until tested prerequisites and the focused command are verified.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A real scan-produced finding is used; waiver state is never seeded
- [ ] #2 Approved waiver request/review/revoke/expiry transitions are asserted against independent facts and authoritative persistence
- [ ] #3 Accepting a waiver never marks the finding remediated or changes Current scan evidence
- [ ] #4 Unauthorized waiver actions are rejected with no side effect
- [ ] #5 The required UI state is covered only when an approved design state exists; otherwise the case is marked blocked
- [ ] #6 Scenario is registered and executed in the complete Assurance owner at a recorded tested commit
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Isolation clarification: TASK-490.16 is a code/setup dependency only; its VM, scan rows, receipts, or outputs cannot be reused. This scenario produces its own real scan finding in its fresh VM as setup, without asserting the complete scan contract a second time and without seeding the finding/waiver. Remain Backlog until a passing/tested foundation dependency and accepted waiver authority exist.
<!-- SECTION:NOTES:END -->
