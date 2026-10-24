---
id: TASK-490.13
title: >-
  Delivery: Attic, S3, and HTTP/Nix cache adapter variants (sequential within
  publication)
status: Backlog
assignee: []
created_date: '2026-10-17 16:37'
updated_date: '2026-10-24 23:10'
labels:
  - testing
  - integration
  - luna
dependencies:
  - TASK-490.12
modified_files:
  - >-
    checks/integration-delivery/scenarios/fragments/task-490.13-publication-variants.py
  - >-
    checks/integration-delivery/scenarios/fragments/task-490.13-publication-variants.cjs
  - checks/integration-delivery/fixtures/publication-variants-*
parent_task_id: TASK-490
priority: medium
type: task
ordinal: 525000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Owning check: integration-delivery. Contract ID: delivery.publication.variants. This packet owns disjoint adapter Python/browser fragments and service fixture definitions under `checks/integration-delivery/scenarios/fragments/task-490.13-*` and owner-local fixtures. Coordinator owns `delivery-lifecycle.nix`, composition, browserSpecs mapping and registration. This is a sequential fragment in one Delivery VM, not a derivation per adapter or packet.

After a real build earlier in the same fresh Delivery run, prove Attic JWT write/scope boundaries and Test behavior, S3 access-key write/read, and HTTP/Nix read-only behavior. Use fresh same-run outputs with `assert_store_absent`; wrong credentials publish nothing where writes exist. Reuse the common workflow and receipts, do not clone full build/publication lifecycle or depend on TASK-490.12 runtime state from another derivation.

Requires TASK-490.7/.10/.12 service/credential code prerequisites and passing foundation/.25/.26. Writable paths are fragments/fixtures only. Production, shared helpers, scenario composition/registration, CI and docs are read-only. Coordinator records one Delivery VM result/evidence.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Attic, S3 and HTTP/Nix each prove their distinct write/read auth boundary using actual same-run outputs
- [ ] #2 Wrong credentials publish nothing for each write-capable adapter
- [ ] #3 No full evaluate/build/retrieve lifecycle is duplicated per adapter
- [ ] #4 The coordinator composes variants sequentially in delivery-lifecycle; no per-adapter derivation is added
- [ ] #5 The coordinator records actual Delivery VM revision, command, outcome and evidence
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-10-17 organization correction: native owner-local paths/commands and explicit coordinator registration. Foundation commit unresolved; two existing Rust package tests block foundation; no VM execution. Backlog, not ready. Future packet needs actual dependency commit(s) and verified commands before To Do. No Luna delegation or executed slice is claimed.

Merge reconciliation preserves source-packet adapter requirements: execute Attic, S3 and HTTP/Nix sequentially within the existing Delivery lifecycle; prove Attic JWT scope negatives and named-cache Test behavior; HTTP/Nix provides public read without a write path. Assert fresh-output absence at each destination before the relevant operation. Metrics internals remain outside integration scope. Do not duplicate the full evaluate/build/retrieve lifecycle per adapter. Canonical fragment ownership and same-run receipt boundaries remain authoritative. This metadata reconciliation does not claim execution or completion.
<!-- SECTION:NOTES:END -->
