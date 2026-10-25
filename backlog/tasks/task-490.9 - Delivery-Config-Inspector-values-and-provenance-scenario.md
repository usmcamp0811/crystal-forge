---
id: TASK-490.9
title: 'Delivery: Config Inspector values and provenance scenario'
status: Backlog
assignee: []
created_date: '2026-10-17 16:36'
updated_date: '2026-10-24 23:07'
labels:
  - testing
  - integration
  - luna
dependencies:
  - TASK-490.8
modified_files:
  - >-
    checks/integration-delivery/scenarios/fragments/task-490.9-config-inspector.py
  - >-
    checks/integration-delivery/scenarios/fragments/task-490.9-config-inspector.cjs
  - checks/integration-delivery/fixtures/config-inspector-*
parent_task_id: TASK-490
priority: medium
type: task
ordinal: 521000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Owning check: integration-delivery. Contract ID: delivery.evaluation.config-inspector. This packet owns only Config Inspector Python/browser fragments and immutable option fixtures: `checks/integration-delivery/scenarios/fragments/task-490.9-config-inspector.{py,cjs}` and `fixtures/config-inspector-*`. Coordinator owns `delivery-lifecycle.nix`, Python composition, browserSpecs mapping and registration; no packet-specific NixOS derivation.

Use a second authored revision created by TASK-490.8 earlier in the same fresh Delivery VM. The system Config tab and flake Config Inspector must show independently authored values/provenance for that exact revision; secret-path values remain redacted and historical selection remains exact. Use same-run commit receipts. No database rows, VM or evaluation output from TASK-490.8's separate historical run are reusable.

Writable paths are the named fragments and immutable owner-local fixtures. Production, shared helper, scenario composition/registration, CI and docs are read-only. Requires TASK-490.7 roles, .8 code, accepted authority, and passing foundation/.25/.26. Coordinator records the one Delivery VM result/evidence.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Inspector values and provenance match authored fixture facts for the exact same-run revision
- [ ] #2 Secret-path values are redacted in API and UI
- [ ] #3 A changed second revision is distinguishable and historical selection stays exact
- [ ] #4 The coordinator composes this fragment into delivery-lifecycle; no packet-specific derivation is added
- [ ] #5 No evaluation or DB output is consumed from another scenario derivation
- [ ] #6 The coordinator records exact revision, command, outcome and evidence for the Delivery VM
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-10-17 organization correction replaces paths/commands with native owner-local layout and coordinator registration. Foundation dependency commit unresolved; two existing Rust package tests block foundation; no VM execution. Backlog, not ready. Actual dependency commit(s) and verified commands required before To Do. No Luna delegation or slice execution is claimed.

Isolation correction: TASK-490.8's runtime commit/evaluation output cannot cross scenario runs. Recreate the authored Git input and evaluate the exact revisions in this scenario's own fresh VM/database; reuse only immutable fixture definitions. Foundation VM is blocked by TASK-490.25/.26; remain Backlog.

Merge reconciliation retains source-packet future acceptance specificity: distinguish option values and definition-file provenance, exercise both System Config and flake Config Inspector, and show partial inventory only when the worker actually reports it. Pure Nix-expression unit behavior remains a lower-level responsibility, not another integration scenario. Preserve canonical exact-revision, redaction, changed-revision/history and same-run isolation criteria. No new execution is claimed.
<!-- SECTION:NOTES:END -->
