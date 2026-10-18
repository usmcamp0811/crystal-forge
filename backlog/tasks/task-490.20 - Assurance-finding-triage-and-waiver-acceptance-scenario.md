---
id: TASK-490.20
title: 'Assurance: real finding triage and batch-triage persistence scenario'
status: Backlog
assignee: []
created_date: '2026-10-17 16:38'
updated_date: '2026-10-18 13:59'
labels:
  - testing
  - integration
  - luna
dependencies:
  - TASK-490.16
modified_files:
  - checks/integration-assurance/scenarios/fragments/task-490.20-triage.py
  - checks/integration-assurance/scenarios/fragments/task-490.20-triage.cjs
parent_task_id: TASK-490
priority: medium
type: task
ordinal: 532000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Owning check: integration-assurance. Contract ID: assurance.remediation.triage. Goal: prove single/fleet and batch triage persistence and exact selection. Risk: medium. Non-goals: waiver (.27), acceptance register (.28), POA&M plan (.21), verification/closure (.29), product changes.

Writable paths (only):
- checks/integration-assurance/scenarios/fragments/task-490.20-triage.py
- checks/integration-assurance/scenarios/fragments/task-490.20-triage.cjs

Composition target: checks/integration-assurance/scenarios/evidence-remediation-lifecycle.nix. Coordinator owns Nix/Python composition, browserSpecs and explicit registration/normal aggregation. After .16 real scans earlier in this fresh Assurance VM/database, consume finding/system identities through receipts. Earlier real import/policy records may provide context. Never consume another derivation's live findings, database, outputs or receipts. No packet-specific derivation and no seeded findings/triage results.

Single/fleet triage and batch triage persist actor, exact scope, status and justification for independently authored selected findings. Unselected findings remain unchanged. Unauthorized Viewer writes fail with no row change. Browser navigation reaches exact CVE/finding/system identity and displays persisted triage state. Expected sets derive from authored facts and producer receipts, never the list response under test.

Dependencies/readiness: TASK-490.16 real finding producer must be integrated with actual tested role/setup prerequisites and same-scenario placement. Foundation bba718befbc183863781656df2df272e5f735d5a on origin/dev 5ae7feab already passed with .25/.26. This packet awaits selection, accepted triage actor/scope/UI authority and coordinator composition; no triage fragment execution is claimed. The prior decomposition is resolved by .27/.28/.29, not an additional unsplit-work blocker.

Read-only sources/authority: cf-server api/cves.rs triage/batch/justification, related queries/models/routes; web-ui CVE views and triage dialogs. Read owner-approved cve-poam-evidence-continuity-design-spec.md only for its approved scope. D1/as-built knowledge is evidence, not acceptance authority. Uncertain cases remain blocked: decision/design gap. Production, shared lib/driver helpers, composition, registration, CI, docs and design are read-only; use documented case/expect/receipt/http/sql_json/run_browser interfaces.

Focused verification after composition: `nix build .#checks.x86_64-linux.integration-assurance.scenarios.evidence-remediation-lifecycle -L`. Coordinator records exact tested revision, command, result and evidence; keep correct failures and linked defects and escalate helper/authority gaps.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Real scan-produced findings from earlier .16 cases in the same Assurance scenario are consumed through receipts; no finding/triage result is seeded or imported from another derivation
- [ ] #2 Single/fleet and batch triage persist actor, exact scope, status and justification for exactly the authored selected set; unselected findings remain unchanged
- [ ] #3 Unauthorized Viewer triage is rejected without a database side effect
- [ ] #4 Browser navigation reaches exact CVE/finding/system identity and displays persisted triage state
- [ ] #5 Coordinator composes only the named triage fragments into evidence-remediation-lifecycle; waiver/acceptance/plan/verification remain separate fragments and no packet-specific derivation is added
- [ ] #6 Coordinator executes the focused evidence-remediation-lifecycle gate and records exact tested revision, command, result and evidence
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Current readiness: bba718be controller foundation passed; .16 producer/setup, accepted triage authority and coordinator composition remain prerequisites. No triage execution is claimed.

### Historical
2026-10-17 reviews recorded foundation blockers and a need to split mixed remediation scope. That split produced .27 waiver, .28 acceptance and .29 verification; .20 is triage only. Earlier separate-run proposals could not reuse .16 live findings. Same-scenario real producer receipts are now permitted.
<!-- SECTION:NOTES:END -->
