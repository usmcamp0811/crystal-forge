---
id: TASK-490.20
title: 'Assurance: finding triage and waiver/acceptance scenario'
status: Backlog
assignee: []
created_date: '2026-10-17 16:38'
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
Owning check: integration-assurance. Contract ID: assurance.remediation.triage. Exclusive write path: tests/integration/assurance/scenarios/remediation-triage.{nix,py,cjs}. Split from POA&M (490.21) because its starting state is untriaged scan evidence and its authority is triage/waiver, not plan execution.

WHY: CVE triage and waivers are proven only by mocked steps (16-cves, 12h) and Rust authorization tests.

SCOPE: starting from real scan evidence produced by the TASK-490.16 fixture: system-level and fleet-level triage with justification persists with actor and scope; batch triage affects exactly the selected findings; a finding waiver is created, reviewed (Accepted/Rejected), expires, and is revoked per WaiverDecision; a waiver never marks the finding remediated; acceptances renew/convert per the acceptance register; Viewer cannot triage (403, no change). Non-goals: POA&M lifecycle (490.21); exports (490.22).

READY WHEN: TASK-490.16 is integrated. Source to read then: cf-server handlers/api/cves.rs (triage, batch-triage, justification), poam.rs (finding-waivers), acceptance_register.rs, models/poam.rs (WaiverDecision); docs/design/CrystalForge cve-poam-evidence-continuity-design-spec.md (owner-approved), docs/knowledge/cves/fleet-cve-triage*; design CveBatchTriage.jsx, PoamViews.jsx.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Triage and batch triage persist with actor and exact scope
- [ ] #2 Waiver decisions follow their lifecycle and never mark remediation
- [ ] #3 Acceptances renew and convert per the register
- [ ] #4 Unauthorized triage is rejected with no change
- [ ] #5 Executed in real VMs with recorded evidence
<!-- AC:END -->
