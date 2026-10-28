---
id: TASK-490.20
title: 'Assurance: real finding triage and batch-triage persistence scenario'
status: In Progress
assignee:
  - '@openai'
created_date: '2026-10-17 16:38'
updated_date: '2026-10-24 05:18'
labels:
  - testing
  - integration
  - luna
dependencies:
  - TASK-490.16
modified_files:
  - checks/integration-assurance/scenarios/fragments/triage.py
  - checks/integration-assurance/scenarios/fragments/triage.cjs
parent_task_id: TASK-490
priority: medium
type: task
ordinal: 532000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Owning check: integration-assurance. Contract: assurance.remediation.triage. Proves single/fleet and batch triage persistence and exact selection from real scan-produced findings in the same fresh Assurance lifecycle. Risk: medium. Non-goals: waiver (.27), acceptance register (.28), plan maintenance (.21), verification/closure (.29), and product changes.

Writable worker paths remain only checks/integration-assurance/scenarios/fragments/triage.py and triage.cjs. Coordinator owns shared composition, browserSpecs, registration, coverage records and normal-owner verification. Never consume another scenario's database, findings, outputs or receipts. Do not seed finding/triage outcomes or add a task-specific derivation.

The existing host Schedule patch journey now executes against real vulnerable Current. It proves actor/scope/disposition/plan/finding/link/baseline identities and Scheduled reload. The bounded continuation reopens and cancels the exact hydrated editor twice without another remediation write and preserves complete authenticated POA&M API detail. Python cross-checks exposed baseline fields against SQL. This does not claim SQL before/after proof for editor reopening. Valid-CSRF Viewer denial remains covered. A separate authenticated Admin missing-POA&M request returns400invalid_poam_payload without changing nine remediation tables, the original scan seal or observations.

Current published checkpointdbf50b4746a6ed3818aed85af68f26251f4b9d52 passed the normal Assurance owner locally with fresh47passing case records/41distinctnames, driver245.18seconds. Its exact-head hosted pipeline2972675961 is pending at the last snapshot. The earlier47pass5ef2d0b0-dirty run remains separately identified. MR335 stays draft and task stays InProgress; no merge/deployment or full triage coverage claim.

Successful fleet/batch mutation and unselected-finding isolation remain open. The present independently authored scanner set contains one pair; additional scans or generations of that same pair are not independent unselected controls. System CVEs batch quick-picks/bar remain a recorded UI gap, not authorization for product edits. Expected selected sets must come from authored facts, never the API list under test.

Authority: approved CVE/POA&M evidence-continuity specification for identity, Current authority, immutable baseline and scope; approved CVE triage design for presentation. Server handlers/services/models and production components provide as-built evidence, not approval for new semantics. Uncertain decisions remain blocked. Production, shared helpers, fixture facts, composition, CI and design remain read-only for this worker increment.

Verification: run the existing evidence-remediation-lifecycle gate through normal integration-assurance owner; record exact revision, actual exit, execution/reuse, browser/SQL evidence and pending hosted checks. Preserve correct failures and downstream .21/.29 state.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Real scan-produced findings from earlier .16 cases in the same Assurance scenario are consumed through receipts; no finding/triage result is seeded or imported from another derivation
- [ ] #2 Single/fleet and batch triage persist actor, exact scope, status and justification for exactly the authored selected set; unselected findings remain unchanged
- [x] #3 Unauthorized Viewer triage is rejected without a database side effect
- [ ] #4 Browser navigation reaches exact CVE/finding/system identity and displays persisted triage state
- [ ] #5 Coordinator composes only the named triage fragments into evidence-remediation-lifecycle; waiver/acceptance/plan/verification remain separate fragments and no packet-specific derivation is added
- [ ] #6 Coordinator executes the focused evidence-remediation-lifecycle gate and records exact tested revision, command, result and evidence
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
User selected a bounded real host-triage contribution in the continuing Assurance lifecycle (2026-10-21), in the existing coordinator worktree/branch under explicit shared composition authority. Real vulnerable builder scan and deployed Current have now executed at 9f53fade-dirty. Add only triage.py/.cjs: actual System CVEs Schedule patch, exact host/CVE/package/source identity, persisted disposition and browser reload; scoped Viewer write rejected without row changes. Reuse fresh same-scenario receipts. Batch/fleet selected-set isolation remains open; no waiver/acceptance or product changes. Coordinator owns composition/registration. Verify via normal Assurance owner and retained browser/SQL evidence.

After the two authorized TASK-490.16 presentation repairs passed the normal Assurance owner locally at clean pushed5ef2d0b0, resume a bounded test-only increment in existing triage.py/.cjs. Extend the actual scheduled-host journey with editor reopen/hydration/cancel and repeated read checks; assert no extra write and unchanged protected remediation/baseline snapshots. Add authorized Admin invalid host schedule payload without POA&M:400invalid_poam_payload and no side effects with valid CSRF. Use existing real same-run finding/actor/source receipts and do not change fixture facts, APIs, product UI, composition or check inventory. Fleet/batch successful selected-set isolation remains open because the current authored set has one pair and System CVEs batch controls have a separate design gap. Coordinator owns verification/publication through the same normal Assurance lifecycle.

Latestownerrequiresfinishauthorizedcoverage, andexistingacceptedreopen/cancelincrement explicitlyrequiresunchangedprotectedSQLsnapshot. Closeonlythisknownproofgap now: split existingcreate versus two existingreopen/cancel browserphases, returningtoPython aftercreate and eachreopen/cancel so fullnine-table remediation snapshots comparebefore/after eachread action. Preserveevery existinghydration/API/baseline/actor/finding/link assertion; fullsnapshotvaluesstayPythonmemory, notlargecontextreceipts. CoordinatoraddscompatiblebrowserSpecsaliasesinexistingAssurancelifecycle (no newwrapperfiles, VMscenario, check, actor orproducer); workerowns onlytriage.py/cjs. Keepbatch/fleet/unselectedset gap explicit; no product/UIchange. Originalcurrent2b54hostedAssurancepassed; newincrementrequiresnewnormalownerandfinalhostedproof, not relabeledoldrun.

Firstintegratedread-boundaryrun18pass1fail: freshreopencontextsignedintoDashboard but retainedoldpage.reload()assumedcreatecontextalreadyonSystems; selectedresponsebodylostacrossnavigation(Protocol Network.getResponseBody). Correcttestsetupbyexplicitexact-SystemCVEsnavigationbeforeoriginalreload. Tieinventory observationtonewrequesteventandconsumeJSONimmediately, preservingactualHTTP200/exactsystem/scan/derivation/current/read-only/finding assertions. Keeporiginalreload/Scheduled/UIhydration/no-write/fullSQL/APIchecks; no retries/sleeps/statuswaivers/productchanges. Retain774n60...artifactthenrunnormalowneragain.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Current readiness: bba718be controller foundation passed; .16 producer/setup, accepted triage authority and coordinator composition remain prerequisites. No triage execution is claimed.

### Historical
2026-10-17 reviews recorded foundation blockers and a need to split mixed remediation scope. That split produced .27 waiver, .28 acceptance and .29 verification; .20 is triage only. Earlier separate-run proposals could not reuse .16 live findings. Same-scenario real producer receipts are now permitted.

Bounded real host-triage contribution executed in current Assurance lifecycle at9f53fade-dirty (46records45passed1historicalcopyfailure overall). triage.py/.cjs create actual System CVEs Schedule patch from real scanned/deployed Current throughbrowser, compareexacthost/CVE/package/actor/disposition/POAMfindinglinkandfullbaseline viaSQL/APIandreload. Real scopedViewer withvalidCSRF cannotremovehostschedule; nine remediation-table snapshots unchanged. Evidence y2nmb68nf9raqmv4ssj07njmxca4nc44.../browser/triage-create and result.json. AC1/3checked. Batch/fleetselectedsetandunselectedisolationremainunimplemented; tasknotReview/Done. Sameinvocationonly; no seeds/cross-scenariofindings.

Resumed by the user's explicit instruction to continue existing Assurance coverage after the two presentation repairs. Existing task-owned coordinator worktree/branch and TASK-490.20 lock retained. Prerequisite producer lifecycle now passes normal integration-assurance owner at clean5ef2d0b0; exact case/evidence audit pending count extraction, no wider .16 completion inferred. Continue only triage.py/.cjs and preserve downstream .21/.29 plan/verification state.

Implemented and published bounded continuation asdbf50b4746a6ed3818aed85af68f26251f4b9d52 in existing draftMR335. Exactly triage.py/.cjs plus coordinator coverage record changed; no product/API/schema/fixture/composition/registration/check-inventory edits. Existing real host Schedule patch journey now reads exact persisted POA&M detail, reopens/cancels the same editor twice with fresh exact-target triageGETs, checks actor/hostscope/schedule/typedowner/date2027-02-01/authoredplan and existing metadata read-only controls, and observes no extra triage/remediation mutation. Full API detail stays equal; exposed baseline/finding/link identities are cross-checked against SQL. This is not SQL before/after proof for the browser reopen phase.

New Admin invalid-host-schedule case actually executes with authenticatedAdmin and validCSRF: missingPOA&M payload returns400invalid_poam_payload; all9protectedremediationtables, originalcompletedscan seal and fullobservations remain unchanged; subsequentexacttriageGETpreserves the schedule. ScopedViewer denial and downstreammaintenance/correctedCurrent/verification/closure remain passing. Successfulfleet/batch application and unselected-finding isolation remainopen; no newauthoredfinding or scopeexpansion inferred.

Normal owner `nix develop --command sh packages/ci/integration-owner-build.sh integration-assurance` passed twice with freshVMs: 5ef2d0b0-dirty47records/41distinctnames,driver246.38s,output39bxp18w8ap718xx0r6f63h9imv7fcz8,logtool_15f079864001VWEEOmUW7YS1io; then cleanpublisheddbf50b4746a6ed3818aed85af68f26251f4b9d52 exited0 with47passed/0failedrecords/41distinctnames,driver245.18s,outputnmhzwmd2dkm0l0jjkq2w4yyid284k33x,logtool_15f163a4b001tYuzXnHsCeUzqc. result.json/meta.json match exactcleanhead. Dirtyresultwasnotrelabeledaftercommit. Node syntax/PythonAST/scopedwhitespace checks passed. Knowledgevalidator58tests/215Mermaid/192concepts passedoutputx14sh5scv4c9k8pv7bh49sspm58b105c.

Final clean-head evidence bundle https://gitlab.com/-/project/70402481/uploads/1e1b9abf4a07f69a1b78eda846ef8747/mr335-assurance-continuation-dbf50b47.tar.gz ; real reopened editor screenshot uploaded inlineMR335 at /uploads/843c2389ff6ad447b87273de66c37c89/triage-host-scheduled-reopened-1.png . No traces/journals/sessionheaders uploaded. MRremainsdraft; currentheadpipeline2972675961pending at singlelastsnapshot, nohostedpassclaimed. TaskInProgressandwiderACsremainopen. Coordinatorpreviewssessionsproduction8090/API3490/taskPG5490anddesign39997areverifiedandretained; no userdatabase touched.
<!-- SECTION:NOTES:END -->
