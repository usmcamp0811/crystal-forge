---
id: TASK-490.21
title: >-
  Assurance: POA&M lifecycle verification, closure, and evidence continuity
  scenario
status: Backlog
assignee: []
created_date: '2026-10-17 16:39'
labels:
  - testing
  - integration
  - luna
dependencies:
  - TASK-490.16
  - TASK-490.18
parent_task_id: TASK-490
priority: medium
type: task
ordinal: 533000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Owning check: integration-assurance. Contract ID: assurance.remediation.poam. Exclusive write path: tests/integration/assurance/scenarios/remediation-poam.{nix,py,cjs}. Split from triage because the starting state is an open plan and the authority is plan execution, verification, and closure.

WHY: legacy POA&M proofs (29g-29m, task433-canonical-poam-lifecycle) start from SQL-seeded FAIL evidence. The contract forbids seeding the result under test; failed evidence must come from a real policy evaluation or scan.

SCOPE: create a POA&M from real failed policy evidence (TASK-490.18 fixture) and from a real CVE finding (TASK-490.16 fixture); link compatible findings; milestones and notes persist; transition through the PoamStatus lifecycle; verification succeeds only after a real remediating deployment changes the evidence (Current evidence becomes passing); closure and reopen preserve finding identity; stale revisions are rejected; rollups by system and bundle reflect the authored set. Non-goals: waiver decisions (490.20); exports (490.22).

READY WHEN: TASK-490.16 and TASK-490.18 are integrated. Source to read then: cf-server handlers/api/poam.rs (transition, verify, close, reopen, milestones, links, rollups), models/poam.rs (PoamStatus, PoamRisk); docs/design/CrystalForge cve-poam-evidence-continuity-design-spec.md (owner-approved), docs/knowledge/poam/*; design PoamsView.jsx, PoamViews.jsx, PoamRegisterParts.jsx.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 POA&Ms are created from real failed evidence, never seeded results
- [ ] #2 Lifecycle transitions, milestones, and links persist with exact finding identity
- [ ] #3 Verification succeeds only after a real remediating deployment changes Current evidence
- [ ] #4 Closure and reopen preserve identity; stale revisions are rejected
- [ ] #5 Executed in real VMs with recorded evidence
<!-- AC:END -->
