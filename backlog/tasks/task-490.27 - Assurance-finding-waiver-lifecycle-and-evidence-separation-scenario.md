---
id: TASK-490.27
title: 'Assurance: finding-waiver lifecycle and evidence separation scenario'
status: Backlog
assignee: []
created_date: '2026-10-17 20:31'
updated_date: '2026-10-19 02:15'
labels:
  - testing
  - integration
  - luna
dependencies:
  - TASK-490.16
modified_files:
  - checks/integration-assurance/scenarios/fragments/waiver.py
  - checks/integration-assurance/scenarios/fragments/waiver.cjs
parent_task_id: TASK-490
priority: high
type: task
ordinal: 540000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Owning check: integration-assurance. Contract ID: assurance.remediation.finding-waiver. Goal: prove the approved waiver lifecycle separately from remediation. Risk: high (decision authority/evidence separation). Non-goals: triage (.20), acceptance-register renewal/conversion (.28), POA&M (.21/.29), product changes.

Writable paths (only):
- checks/integration-assurance/scenarios/fragments/waiver.py
- checks/integration-assurance/scenarios/fragments/waiver.cjs

Composition target: checks/integration-assurance/scenarios/evidence-remediation-lifecycle.nix. Coordinator owns Nix/Python composition, browserSpecs and explicit registration/normal aggregation. Use real .16 scan-produced findings earlier in this fresh Assurance VM/database through receipts. Earlier real same-scenario findings MAY be consumed; another derivation's live findings, outputs, rows or receipts MUST NOT be consumed. No packet-specific derivation; never seed findings or waiver states. No additional fixture path is granted; author needed waiver facts in these fragments.

Starting with a distinct real finding, an approved actor submits a waiver request and an authorizer reviews Accepted or Rejected as applicable; revoke an accepted waiver. Assert expiry using an authored due time or deterministic clock only if an existing approved API/test boundary supports it; do not add test clocks. Independently read persistence and UI. Accepted/rejected/revoked/expired decisions remain separate from remediation: waiver acceptance never marks the finding remediated, changes scan evidence or changes Current status. Unauthorized actions/review have no side effect. Exact actor/scope/UI and transitions must be approved; otherwise retain blocked: decision/design gap.

Dependencies/readiness: TASK-490.16 real finding producer and its tested role/setup interfaces must be integrated at recorded revisions. Foundation bba718befbc183863781656df2df272e5f735d5a on origin/dev 5ae7feab passed with .25/.26. Await selection, accepted waiver/expiry/actor/UI authority and coordinator ordering. Foundation startup failures and prior mixed remediation scope are historical. No waiver fragment run is claimed.

Authority/read-only sources: first read docs/design/CrystalForge/docs/crystal-forge-systems-design/cve-poam-evidence-continuity-design-spec.md and confirm approval covers these exact interactions. docs/knowledge/cves/* and poam/* are as-built evidence only. Inspect cf-server handlers/api/poam.rs finding-waiver routes, models/poam.rs WaiverDecision, server router; UI waiver/modal caller and approved PoamViews.jsx/CveBatchTriage.jsx. Do not infer requirements from enum variants. Use documented case/expect/expect_equal/receipt/http/sql_json/run_browser interfaces; production, shared helpers, composition, registration, CI, docs and design are read-only.

Focused verification after composition: `nix build .#checks.x86_64-linux.integration-assurance.scenarios.evidence-remediation-lifecycle -L`. Coordinator records exact tested revision, command, result and evidence. Preserve correct failures and linked defects; escalate helper/authority gaps.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Real .16 scan-produced findings earlier in the same Assurance scenario are consumed through receipts; no finding/waiver state is seeded or read from another derivation
- [ ] #2 Approved request, Accepted/Rejected review, revoke and expiry transitions are proved against independent facts and persistence; expiry uses only an existing approved boundary, with no new test clock
- [ ] #3 Accepting a waiver never marks the finding remediated or changes scan evidence/Current status; other waiver decisions remain distinct from remediation
- [ ] #4 Unauthorized waiver actions/review are rejected with no side effect
- [ ] #5 Required actor/scope/UI states are exercised only under approved authority; unresolved cases stay blocked: decision/design gap; coordinator composes only named fragments into evidence-remediation-lifecycle without a packet derivation
- [ ] #6 Coordinator executes the focused evidence-remediation-lifecycle gate and records exact tested revision, command, result and evidence
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Current readiness: bba718be controller foundation passed. .16 real findings/setup and accepted waiver authority/expiry/UI remain prerequisites for coordinator selection/composition. No waiver run is claimed.

### Historical
This packet was split from mixed triage/acceptance scope on 2026-10-17. Earlier isolation notes referred to separate scenario runs and unresolved foundation startup. They prohibited cross-derivation findings, not the current use of earlier real .16 cases through receipts inside one Assurance lifecycle.
<!-- SECTION:NOTES:END -->
