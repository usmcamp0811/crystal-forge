---
id: TASK-490.21
title: 'Assurance: create and maintain POA&M plans from real failed evidence'
status: In Progress
assignee:
  - '@openai'
created_date: '2026-10-17 16:39'
updated_date: '2026-10-21 08:22'
labels:
  - testing
  - integration
  - luna
dependencies:
  - TASK-490.16
  - TASK-490.18
modified_files:
  - checks/integration-assurance/scenarios/fragments/poam-plan.py
  - checks/integration-assurance/scenarios/fragments/poam-plan.cjs
parent_task_id: TASK-490
priority: medium
type: task
ordinal: 533000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Owning check: integration-assurance. Contract ID: assurance.remediation.poam-plan. Goal: create and maintain POA&M plans from real failed evidence. Risk: high (evidence links and transaction integrity). Non-goals: verification/closure/reopen (.29), triage (.20), waiver/acceptance (.27/.28), product changes.

Writable paths (only):
- checks/integration-assurance/scenarios/fragments/poam-plan.py
- checks/integration-assurance/scenarios/fragments/poam-plan.cjs

Composition target: checks/integration-assurance/scenarios/evidence-remediation-lifecycle.nix. Coordinator owns Nix/Python composition, browserSpecs and explicit registration/normal aggregation. Compose after .16/.18 real failed CVE/policy evidence earlier in one fresh Assurance VM/database. Earlier real findings/evaluations MAY be consumed through receipts. Another derivation's live rows, outputs, generations or receipts MUST NOT be consumed. No packet-specific derivation; never seed failed evidence, findings, plans or generated identities.

Create/link plans through the production workflow. Exact finding/evidence/plan identity must survive authoritative read and browser reload. Notes, milestones, links and approved nonterminal plan edits/transitions persist. Failed create and stale revision leave no partial or persisted side effects where accepted requirements specify the state; uncertain component/state authority remains blocked. .29 consumes the actual plans through receipts in this same scenario, after real remediation/Current evidence changes, rather than expanding this packet to closure.

Dependencies/readiness: TASK-490.16 and TASK-490.18 must supply real failed evidence through integrated setup/role interfaces at recorded tested revisions. Controller foundation bba718befbc183863781656df2df272e5f735d5a on origin/dev 5ae7feab passed with .25/.26. Await selection, approved plan-state/failure authority and coordinator composition. Historical startup failure and the already-resolved remediation split are not current blockers. No plan fragment run is claimed.

Read-only sources/authority: cf-server api/poam.rs transitions/verify/close/reopen/milestones/links/rollups, models/poam.rs PoamStatus/PoamRisk; owner-approved docs/design/CrystalForge/docs/crystal-forge-systems-design/cve-poam-evidence-continuity-design-spec.md; docs/knowledge/poam/* as as-built evidence only; approved PoamsView.jsx/PoamViews.jsx/PoamRegisterParts.jsx. Do not infer accepted transitions from enums or current output. Production, shared helpers, scenario composition, registration, CI, docs and design are read-only; use documented case/assert/receipt/http/sql_json/run_browser interfaces.

Focused verification after composition: `nix build .#checks.x86_64-linux.integration-assurance.scenarios.evidence-remediation-lifecycle -L`. Coordinator records exact tested revision, command, result and evidence. Preserve correct failures and linked product defects; escalate helper/authority gaps.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Plans start from real failed CVE/policy evidence produced earlier by .16/.18 in the same Assurance scenario; no failed evidence/finding/plan is seeded
- [x] #2 POA&M creation/linking preserves exact finding/evidence/plan identities in stored links and UI through reload
- [x] #3 Milestones, notes, links and approved nonterminal plan-state transitions persist
- [ ] #4 Stale updates and failed plan creates have no partial or persisted effects where accepted authority defines those states; exact unresolved states remain blocked
- [ ] #5 Coordinator composes named plan fragments with .29 verification/closure in evidence-remediation-lifecycle using same-run receipts, with no packet derivation or cross-derivation live state
- [ ] #6 Coordinator executes the focused evidence-remediation-lifecycle gate and records exact tested revision, command, result and evidence
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
User selected bounded CVE POA&M contribution to same fresh evidence-remediation-lifecycle in existing coordinator worktree/branch. Use .20 real host Schedule patch from .16 actual vulnerable Current, never seeded finding/plan. Add poam-plan.py/.cjs for exact finding/evidence link and persisted authored plan, approved in_progress transition, note and milestone. Check real API plus browser reload; preserve actor, scope and original baseline across built-clean and actual corrected activation. Policy-family creation and broad failure/stale-edit matrix remain open. No production edits. Coordinator normal owner gate supplies execution evidence.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Current readiness: controller foundation bba718be passed. .16/.18 failed-evidence producers, approved plan transitions/failure states and coordinator composition remain prerequisites. No plan fragment execution is claimed.

### Historical
2026-10-17 reviews recorded unresolved package/startup failures and mixed remediation scope. The decomposition assigned plan entry to .21 and verification/closure/reopen to .29; .20/.27/.28 own triage/waiver/acceptance. Legacy 29g-29m/canonical POA&M cases used SQL-seeded FAIL evidence. Current cases require real producer evidence and may share same-scenario receipts, never another derivation's live state.

Bounded real CVE plan contribution executed at9f53fade-dirty in Assurance normalowner (overall45passingrecords/1historicalcopyfailure). ActualUIhosttriagecreatesplanandlink; exactstablefinding/plan/link/allbaseline_* fields surviveAPI/SQL/browserreload. ProductionAPIupdatedauthoredplan,in_progressstatus,noteandmilestonepersist; browserpoam-maintainchecksrerender/reload. FuturecleanbuildleavesaffectedCurrentandnoncompletedplan/originalbaseline; correctedactivationalone doesnotcloseplan. Originalfindingbaseline survivesexplicitverification/closureby.29. Evidence y2nmb68nf9raqmv4ssj07njmxca4nc44.../browser/{triage-create,poam-maintain,poam-verify-close}. AC2/3checked forselectedCVElifecycle; policy-familycreation,broadstale/failedcreateandremainingcriteriaopen. No browser-edit-control claim forAPIwrites.
<!-- SECTION:NOTES:END -->
