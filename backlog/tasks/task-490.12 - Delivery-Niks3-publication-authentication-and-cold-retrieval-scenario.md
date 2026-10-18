---
id: TASK-490.12
title: 'Delivery: Niks3 publication, authentication, and cold retrieval scenario'
status: Backlog
assignee: []
created_date: '2026-10-17 16:37'
updated_date: '2026-10-18 03:26'
labels:
  - testing
  - integration
  - luna
dependencies:
  - TASK-490.10
modified_files:
  - checks/integration-delivery/scenarios/fragments/task-490.12-niks3.py
  - checks/integration-delivery/scenarios/fragments/task-490.12-niks3.cjs
parent_task_id: TASK-490
priority: high
type: task
ordinal: 524000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Owning check: integration-delivery. Contract ID: delivery.publication.niks3. This packet owns disjoint Niks3 Python/browser fragments under `checks/integration-delivery/scenarios/fragments/task-490.12-*`. Coordinator owns `delivery-lifecycle.nix`, composition, browserSpecs mapping and registration; no packet-specific VM derivation.

Within the same fresh Delivery VM, configure cache through the UI, use a real output produced earlier in the scenario, publish with token+mTLS write auth, independently verify the destination, and prove cold targetA retrieval/signature verification after asserting output absence. Wrong token/client certificate publishes nothing; another environment receives no read credential. Same-run receipts bind output/cache/target identities. TASK-490.13 adds other adapters sequentially in this VM, and .14/.15 use real target state from this scenario.

Requires TASK-490.7/.10 roles and accepted authority, plus passing foundation/.25/.26. Writable paths are Niks3 fragments only. Production, helpers, composition, registration, CI and docs are read-only. Coordinator records the single Delivery VM run and evidence.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A fresh same-run output is published to Niks3 with token+mTLS write auth and verified by an independent read
- [ ] #2 Cold targetA retrieves it with signature verification after independent absence assertion
- [ ] #3 Wrong credentials publish nothing; another environment gets no read credential
- [ ] #4 The coordinator composes this fragment into delivery-lifecycle and retains legacy Niks3 gates until cutover
- [ ] #5 No other scenario's output, target or DB is consumed
- [ ] #6 The coordinator records actual Delivery VM evidence
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-10-17 organization correction replaces prior paths/commands with native owner-local layout. Coordinator owns registration/shared lib/CI/docs. Foundation commit unresolved; foundation blocked by two existing Rust package tests; no VM execution. Backlog, not ready; actual dependency commit(s) and verified commands required before To Do. No Luna delegation or executed slice is claimed.
<!-- SECTION:NOTES:END -->
