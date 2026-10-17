---
id: TASK-490.20
title: 'Assurance: real finding triage and batch-triage persistence scenario'
status: Backlog
assignee: []
created_date: '2026-10-17 16:38'
updated_date: '2026-10-17 20:33'
labels:
  - testing
  - integration
  - luna
dependencies:
  - TASK-490.16
parent_task_id: TASK-490
priority: medium
type: task
ordinal: 532000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Owning check: integration-assurance. Contract ID: assurance.remediation.triage. Exclusive scenario writes: checks/integration-assurance/scenarios/remediation-triage.{nix,py,cjs}.

Parent: TASK-490. Dependencies: TASK-490.16 for real scan evidence, plus the tested native-harness dependency commit from TASK-490.1 (commit not yet proven; remains Backlog). No foundation/child commit is implied by the historical 33de73b baseline.

Objective: starting from real CVE scan findings produced by TASK-490.16, test only finding triage and batch triage. Actor: Admin or Operator permitted by accepted authority. Action: triage a finding with justification; batch-triage a deliberately distinguishable selected set. Expected: the persisted actor, status, scope, and justification equal authored facts; unselected findings remain unchanged; Viewer is denied and no row changes. Browser CVE/System detail navigation reaches the exact finding identity and shows the saved triage state. Non-goals: finding waivers or acceptance renew/convert (separate packets), POA&M lifecycle, exports, production changes.

Authority: owner-approved docs/design/CrystalForge cve-poam-evidence-continuity-design-spec.md only for its specified continuity requirements; read it and verify status/scope. Read docs/knowledge/cves/fleet-cve-triage*. As-built D1 is not acceptance authority; any expectation without approved authority remains blocked. Production sources to read: cf-server src/handlers/api/cves.rs (system triage, batch-triage, justification), relevant queries/models and router; packages/web-ui CVE views/triage dialog; design CvesView.jsx, CveBatchTriage.jsx, SystemDetail.jsx.

Starting facts must be actual scan results, never SQL-seeded outcomes. Reuse helpers from lib.crystal-forge.makeIntegrationTestHarness { pkgs, inputs }; controller/services roles and signatures are coordinator-owned in lib/integration-test. At handoff, scenario module signature is {harness, owner, name}; Python helpers include case, expect, expect_equal, http, sql_json, run_browser; browser ctx includes page, expect, facts, receipts, emit, url, step. Explicit coordinator registration in checks/integration-assurance/default.nix is required after implementation; no custom runner.

UI obligations: success and denied/no-side-effect states; exact CVE/system/scan identity and return navigation. Evidence: DB before/after rows with expected facts independent of API output; browser trace/screenshots; run result and journals. No visual comparison unless the approved reference includes the exact state.

Writable paths only: checks/integration-assurance/scenarios/remediation-triage.{nix,py,cjs} and checks/integration-assurance/fixtures/triage-* if needed. Read-only: every other path, especially lib/integration-test/**, owner default.nix, flake/CI, packages/, modules/, schemas/, docs/design/CrystalForge/. Coordinator explicitly registers the scenario; worker does not edit shared files.

Focused command after actual dependency integration: `nix build .#checks.x86_64-linux.integration-assurance.scenarios.remediation-triage -L`; `.run` is evidence only. These commands have NOT been verified for this scenario. Complete owner command: `nix build .#checks.x86_64-linux.integration-assurance -L`, owned/run by coordinator. One VM run at a time until capacity is measured.

Completion: named scenario is registered; real VM run passes all cases or retains correct failing assertions with a linked Backlog defect; evidence stores source revision, scenario, actor, independent facts, negative side effect, output path; task notes name commands and outcomes. Escalate accepted-authority or shared-interface conflicts to the TASK-490 coordinator. This child remains Backlog and is not dispatch-ready while its tested foundation/dependency commits, production defect prerequisites, or accepted authority remain unresolved.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The scenario begins with real scan-produced CVE findings and does not seed triage results
- [ ] #2 Single and batch triage persist actor, scope, status, and justification for exactly the authored finding set; unselected findings remain unchanged
- [ ] #3 Unauthorized Viewer triage is rejected with no database side effect
- [ ] #4 Browser navigation reaches the exact finding/system identity and reports the persisted triage state
- [ ] #5 The scenario is explicitly registered by the coordinator and executes in the complete Assurance owner run
- [ ] #6 The focused scenario runs in a real VM; commands, tested revision, result, and evidence paths are recorded
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-10-17 organization correction replaces old layout/commands. Additional dispatch blocker: TASK-490.20/.21 need finer splitting of triage versus policy waiver/acceptance versus closure/verification. Scope preserved, not expanded or split now. Foundation dependency commit unresolved; foundation blocked by two existing Rust package tests; no VM execution. Backlog, not ready. Actual dependency commit(s) and verified commands required before To Do. No Luna delegation or executed slice is claimed.

Packet decomposition correction 2026-10-17: this task now covers triage/batch triage only. Do not test or own finding waiver or acceptance-register lifecycle; those are separate TASK-490.27/.28 with distinct starting state/authority. Every run is a fresh VM/database, so TASK-490.16's previous run output cannot be reused; execute a real scan as setup in this scenario using the accepted setup flow, never seed findings. Foundation scenario has executed and failed on TASK-490.25/.26; no ready foundation dependency commit exists. Remain Backlog.
<!-- SECTION:NOTES:END -->
