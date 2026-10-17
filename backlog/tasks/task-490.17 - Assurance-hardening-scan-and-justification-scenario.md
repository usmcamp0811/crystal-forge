---
id: TASK-490.17
title: 'Assurance: hardening scan and justification scenario'
status: Backlog
assignee: []
created_date: '2026-10-17 16:38'
labels:
  - testing
  - integration
  - luna
dependencies:
  - TASK-490.14
parent_task_id: TASK-490
priority: medium
type: task
ordinal: 529000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Owning check: integration-assurance. Contract ID: assurance.scanning.hardening. Exclusive write path: tests/integration/assurance/scenarios/scanning-hardening.{nix,py,cjs}. Separate from CVE scanning because it uses the hardening worker and target service facts, not vulnerability data.

WHY: hardening views are mocked (27, 28) and marked knownIssue in the legacy manifest.

SCOPE: targetA runs authored services with deliberately different systemd hardening settings; a hardening scan reports the expected per-service findings (expected values come from the authored unit settings, not from CF output); a justification persists and is shown after reload; a rescan after a deployment that changes one service updates only that service. Non-goals: STIG controls (490.19); CVEs.

READY WHEN: TASK-490.14 is integrated. Source to read then: cf-server bin/hardening-worker.rs, handlers/api/hardening.rs, systems.rs (hardening, hardening-inventory, hardening-scan, justifications); modules/nixos/crystal-forge hardening options; design HardeningTab.jsx.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Hardening findings match authored per-service unit settings on a real target
- [ ] #2 Justifications persist through reload
- [ ] #3 A rescan after a real deployment updates only the changed service
- [ ] #4 Executed in real VMs with recorded evidence
<!-- AC:END -->
