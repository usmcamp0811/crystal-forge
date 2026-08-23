---
id: TASK-433.7
title: >-
  TASK-433 Phase 6: POA&M integration in evidence/finding, system, bundle,
  assignment UI
status: Backlog
assignee: []
created_date: '2026-08-23 01:43'
labels:
  - design-parity
  - poam
  - web-ui
  - compliance
  - phase-6
dependencies:
  - TASK-433.6
references:
  - TASK-433
  - TASK-433.1
documentation:
  - docs/design/CrystalForge/components/ComplianceView.jsx
  - docs/design/CrystalForge/components/SystemDetail.jsx
parent_task_id: TASK-433
priority: high
type: feature
ordinal: 439000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Objective
Parent umbrella: TASK-433, phase 6 of 8 (contextual only). Wires the Phase 5 POA&M API into compliance/evidence, system detail, and bundle compliance views, and makes assignment POA&M references first-class relationships.

## Explicit scope
- Compliance/evidence: failed rows stay FAIL and expose Create/Link POAM with exact navigation to/from finding/bundle/system/evidence, with real prefilled context.
- System compliance: real committed POAM filters/counts and exact finding navigation.
- Bundle compliance: real open/on-POAM/no-POAM/overdue/awaiting-verification/closed rollups with no N+1 visible-list queries (uses Phase 5 batched rollup APIs).
- Assignment POAM references are first-class relationships and do not mutate immutable assignment versions.

## Explicit non-scope
No dashboard/notifications/coach work (Phase 7). No POA&M API changes (Phase 5 owns the API; this subtask consumes it).

## Verification
```bash
nix develop -c cargo fmt --manifest-path packages/web-ui/Cargo.toml -- --check
nix develop -c cargo test --manifest-path packages/web-ui/Cargo.toml
nix build .#packages.x86_64-linux.web-ui --no-link
nix build .#checks.x86_64-linux.web-ui --no-link
```
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Evidence supports Create POAM and Link existing with real prefilled context and exact navigation to/from finding/bundle/system/evidence.
- [ ] #2 System compliance provides real committed POAM filters/counts and exact finding navigation.
- [ ] #3 Bundle compliance provides real open/on-POAM/no-POAM/overdue/awaiting-verification/closed rollups and no N+1 visible-list queries.
- [ ] #4 Assignment POAM references are first-class relationships and do not mutate immutable assignment versions.
<!-- AC:END -->
