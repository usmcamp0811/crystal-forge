---
id: TASK-490.17
title: 'Assurance: hardening scan and justification scenario'
status: Backlog
assignee: []
created_date: '2026-10-17 16:38'
updated_date: '2026-10-19 02:13'
labels:
  - testing
  - integration
  - luna
dependencies:
  - TASK-490.14
modified_files:
  - checks/integration-assurance/scenarios/fragments/hardening.py
  - checks/integration-assurance/scenarios/fragments/hardening.cjs
parent_task_id: TASK-490
priority: medium
type: task
ordinal: 529000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Owning check: integration-assurance. Contract ID: assurance.scanning.hardening. Goal: prove real hardening worker results from independently authored target unit facts. Risk: medium. Non-goals: CVE assertions, STIG controls (.19), product changes.

Writable paths (only):
- checks/integration-assurance/scenarios/fragments/hardening.py
- checks/integration-assurance/scenarios/fragments/hardening.cjs

Composition target: checks/integration-assurance/scenarios/evidence-remediation-lifecycle.nix. Coordinator owns Nix/Python composition, browserSpecs and explicit owner registration/normal aggregation. Compose alongside CVE/policy/import/remediation/export fragments in one fresh Assurance VM/database. Earlier real target/deployment results in this scenario MAY be consumed through receipts; another derivation's live target, generation, database, output or receipts MUST NOT be consumed. No packet-specific derivation; keep authored unit facts in these fragments and coordinate target configuration through the coordinator.

targetA runs authored services with distinct systemd hardening settings. Run the real hardening scan and compare per-service findings to independently authored unit settings, not CF output. Persist justifications and prove reload. Perform a real deployment changing one service, rescan and prove only that service's result updates while unrelated services remain unchanged. Expected target identity comes from same-run receipts and target observations. Never seed hardening findings or justification outcomes.

Dependencies/readiness: TASK-490.14 deployment setup and its .7 target/services role interfaces must be integrated at actual tested revisions; these are code/setup prerequisites, not state sharing. Foundation bba718befbc183863781656df2df272e5f735d5a on origin/dev 5ae7feab already passed with .25/.26. This packet awaits selection, concrete deployment/unit setup, accepted hardening/justification expectations and coordinator composition. Historical startup defects are not unresolved prerequisites.

Read-only source/authority: cf-server bin/hardening-worker.rs, api/hardening.rs, systems.rs hardening/inventory/scan/justifications, NixOS hardening options and approved HardeningTab.jsx. As-built D1 is evidence, not accepted authority; uncertain cases remain blocked pending owner decision. Production, shared helpers, scenario composition/target configuration, registration, CI, docs and design references are read-only.

Focused verification after composition: `nix build .#checks.x86_64-linux.integration-assurance.scenarios.evidence-remediation-lifecycle -L`. Coordinator records exact tested revision, command, outcome and evidence. No hardening fragment execution is claimed. Preserve correct failures and linked product defects; escalate helper/authority gaps.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Real targetA hardening findings match independently authored distinct per-service unit settings, without seeded scan results
- [ ] #2 Justifications persist through database read and browser reload
- [ ] #3 After a real deployment changing one service, rescan updates only that service and leaves unrelated services unchanged
- [ ] #4 Coordinator composes only the named hardening fragments into evidence-remediation-lifecycle; same-scenario real setup may be consumed through receipts, with no separate packet derivation or cross-derivation live state
- [ ] #5 Coordinator executes the focused evidence-remediation-lifecycle gate and records exact tested revision, command, outcome and evidence
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Current readiness: bba718be controller foundation passed. Real deployment/target setup from .14/.7, authored unit settings, accepted authority and coordinator composition remain prerequisites; no hardening run is claimed.

### Historical
The 2026-10-17 organization/isolation review recorded unresolved package/startup prerequisites at that time. A separate hardening scenario could not reuse Delivery live state. Current same-scenario fragments may use earlier real Assurance target/deployment results through receipts.
<!-- SECTION:NOTES:END -->
