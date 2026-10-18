---
id: TASK-490.8
title: >-
  Delivery: Git ingestion, event deduplication, and exact evaluation identity
  scenarios
status: Backlog
assignee: []
created_date: '2026-10-17 16:36'
updated_date: '2026-10-18 03:26'
labels:
  - testing
  - integration
  - luna
dependencies:
  - TASK-490.7
modified_files:
  - checks/integration-delivery/scenarios/fragments/task-490.8-ingestion.py
  - checks/integration-delivery/scenarios/fragments/task-490.8-ingestion.cjs
  - checks/integration-delivery/scenarios/fragments/task-490.8-identity.py
  - checks/integration-delivery/scenarios/fragments/task-490.8-identity.cjs
  - checks/integration-delivery/fixtures/evaluation-*
parent_task_id: TASK-490
priority: high
type: task
ordinal: 520000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Owning check: integration-delivery. Contract IDs: delivery.evaluation.ingestion and delivery.evaluation.identity. This packet owns disjoint Python/browser fragments and immutable Git fixture definitions only: `checks/integration-delivery/scenarios/fragments/task-490.8-{ingestion,identity}.{py,cjs}` and owner-local `evaluation-*` fixtures. The coordinator owns `delivery-lifecycle.nix`, Python composition, browserSpecs mapping, and Delivery registration. Do not create per-packet NixOS derivations.

Within the same fresh Delivery VM, push/poll/webhook authored commits, prove deduplication and force-push, then independently compute drvPaths and verify exact history in the UI. Reuse same-run commits and identities by receipts; later Config Inspector/build/publication/activation cases may use those same real records. No runtime DB, VM, output or receipt may come from another derivation. Dependencies TASK-490.7 supplies roles/setup code only, not live rows.

Writable paths are named fragments and owner-local immutable Git fixtures. Production code, shared helpers, scenario composition/registration, CI and docs are read-only. D1 uncertain behavior remains blocked. Requires passing foundation and TASK-490.25/.26 and TASK-490.7 roles/proving interface. Coordinator records the single Delivery VM result/evidence.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Each authored commit is ingested exactly once; replays are deduplicated and force-push rewrite is surfaced
- [ ] #2 Recorded drvPaths equal independent nix eval results for distinguishable commits
- [ ] #3 The UI shows exact commit/evaluation identity and keeps history exact when head moves
- [ ] #4 The coordinator composes both fragments into delivery-lifecycle; no packet-specific derivation is added
- [ ] #5 The Delivery scenario creates its own Git data and does not consume another run's output
- [ ] #6 The coordinator records exact tested revision, command, outcome and evidence
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-10-17 organization correction: native owner-local paths and explicit coordinator registration replace old layout. Child files are disjoint from coordinator proving scenario. Foundation commit unresolved; two existing Rust package tests block foundation; no VM execution. Backlog, not ready. Future packet needs actual dependency commit(s) and verified commands before To Do. No Luna delegation or executed slice is claimed.

Isolation correction: do not consume runtime Git commit/evaluation records from TASK-490.7. Authored immutable Git fixture definitions may be reused, but this scenario pushes/ingests its own commits in its fresh VM/database and records its own receipts. Current foundation VM fails at server startup; remain Backlog until corrected foundation evidence and tested role-proving commit exist.
<!-- SECTION:NOTES:END -->
