---
id: TASK-490.28
title: 'Assurance: acceptance-register renewal, conversion, and scope scenario'
status: Backlog
assignee: []
created_date: '2026-10-17 20:31'
updated_date: '2026-10-18 14:00'
labels:
  - testing
  - integration
  - luna
dependencies:
  - TASK-490.27
  - TASK-490.21
modified_files:
  - checks/integration-assurance/scenarios/fragments/task-490.28-acceptance.py
  - checks/integration-assurance/scenarios/fragments/task-490.28-acceptance.cjs
parent_task_id: TASK-490
priority: medium
type: task
ordinal: 541000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Owning check: integration-assurance. Contract ID: assurance.remediation.acceptance-register. Goal: prove source-owned acceptance projection, scope and selected-record renew/convert behavior. Risk: high (cross-record identity and transactions). Non-goals: triage/waiver lifecycle assertions (.20/.27), POA&M verification (.29), product changes.

Writable paths (only):
- checks/integration-assurance/scenarios/fragments/task-490.28-acceptance.py
- checks/integration-assurance/scenarios/fragments/task-490.28-acceptance.cjs

Composition target: checks/integration-assurance/scenarios/evidence-remediation-lifecycle.nix. Coordinator owns Nix/Python composition, browserSpecs and explicit registration/normal aggregation. After approved .27/.21 source flows produce real source-owned acceptances earlier in this fresh Assurance VM/database, consume exact source/acceptance IDs through receipts. Earlier same-scenario real records MAY be reused; another derivation's live rows, outputs or receipts MUST NOT be consumed. No packet-specific derivation; never seed acceptance/waiver/plan records or renew/convert results.

Authorized Admin/Operator starts with one real Accepted source record and one different source record. UI list/filter results preserve source identity and exact scope. Renew changes only the selected acceptance, persisting approved review/expiration fields. Convert creates a POA&M with exact source/evidence links, without duplication or mutation of another acceptance. Unauthorized actions leave rows unchanged. Browser return navigation retains originating finding/plan identity. Expected fields/sets are independently authored or producer-action receipts, never the list response under test. Unapproved source kinds/actor/transition/UI semantics stay blocked: decision/design gap.

Dependencies/readiness: TASK-490.27 and TASK-490.21 source producer semantics/code and their tested real evidence/setup interfaces must be integrated at recorded revisions. Foundation bba718befbc183863781656df2df272e5f735d5a on origin/dev 5ae7feab passed with .25/.26. Await selection, accepted source limits/renew/convert authority and coordinator ordering; historical startup defects are resolved. No acceptance-register fragment run is claimed.

Read-only authority/source: accepted owner specifications define exact source limits; docs/knowledge/poam/* is as-built evidence only. Read cf-server handlers/api/acceptance_register.rs list/renew/convert/request types, queries/acceptance_register.rs AcceptanceSource/scope, services/poam.rs renew/convert transactions and router; UI acceptance-register/PoA&M views. Approved design component controls presentation, else record visual design gap. Use documented case/assert/receipt/http/sql_json/run_browser interfaces; production, shared helpers, composition, registration, CI, docs and design are read-only.

Focused verification after composition: `nix build .#checks.x86_64-linux.integration-assurance.scenarios.evidence-remediation-lifecycle -L`. Coordinator records exact tested revision, command, result and evidence. Keep correct failures and linked defects; escalate helper/authority gaps.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The register begins with real source-owned acceptances from approved earlier .27/.21 flows in the same Assurance scenario; no acceptance/result is seeded or consumed from another derivation
- [ ] #2 UI list/filter exact source identity and scope match independently authored facts or producer receipts
- [ ] #3 Renew affects only the selected acceptance and persists approved review/expiration fields
- [ ] #4 Convert creates a POA&M preserving exact source/evidence identity without duplicate or mutation of another acceptance
- [ ] #5 Unauthorized actions have no side effects and browser return navigation preserves originating finding/plan identity; unresolved source/transition/UI authority stays blocked
- [ ] #6 Coordinator composes only named acceptance fragments into evidence-remediation-lifecycle, executes its focused gate without a packet-specific derivation, and records exact tested revision, command, result and evidence
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Current readiness: bba718be controller foundation passed. Real .27/.21 source producers and accepted source/renew/convert authority remain prerequisites for selection and coordinator ordering. No register fragment execution is claimed.

### Historical
The acceptance register was split from triage/waiver because its projection, source state and actors differ. Earlier isolation notes assumed independent runs and unresolved foundation startup. Current fragments consume earlier real same-scenario source records through receipts, never another derivation's live state.
<!-- SECTION:NOTES:END -->
