---
id: TASK-433.7
title: >-
  TASK-433 Phase 6: POA&M integration in evidence/finding, system, bundle,
  assignment UI
status: In Progress
assignee:
  - '@opencode-agent'
created_date: '2026-08-23 01:43'
updated_date: '2026-08-27 03:24'
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

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Preflight: rebased `TASK-433-policy-poam-workflows` onto current `origin/dev` (`79153f74`) without discarding later legitimate changes. Replayed Phase 1-5 implementation; Phase-5 commit is now `42488e89` (rebased equivalent of accepted `68904343`) and remains the branch head before Phase 6. TASK-433.6 is Review with all six ACs checked and successful exact-head pipeline 2794857147 recorded. Dedicated worktree is `/home/mcamp/code/crystal-forge/TASK-433-policy-poam-workflows`; scope is exactly AC1-AC4, with Phase 7 dashboard/notifications/coach explicitly excluded.

Phase-6 contract audit found blocking Phase-5 API omissions before any implementation write. AC1 cannot be implemented without client-side approximation/N+1 because evidence DTOs expose only an assessment ID and no stable finding/remediation relationship, while Phase 5 has no batch active/historical remediation lookup for visible findings/assessment IDs. AC1 Link Existing is also directionally unsupported: `GET /poams/:poam_id/compatible` returns findings compatible with a known POA&M, but the required finding-origin flow needs server-filtered POA&Ms compatible with a supplied real finding/assessment; broad `/poams?policy_lineage_id=...` listing would shift compatibility/active-link decisions into the browser. AC4 cannot render first-class relationships from the assignment surface because there is no assignment-version filter or batch lookup; assignment references are discoverable only after fetching an already-known POA&M detail. These gaps belong to TASK-433.6 AC2 and its explicit Phase-5 scope for authenticated compatible search and batched active links/assignment relationships. Per the user's stop rule, no Phase-6 code or tests were written. Required Phase-5 correction would expose bounded authorization-scoped contracts for (1) active plus completed POA&M relationships by assessment/finding IDs, (2) compatible POA&M search by authoritative assessment/finding context, and (3) POA&M relationships by immutable assignment-version IDs. Awaiting explicit authorization to amend Phase 5 or another server-contract decision.
<!-- SECTION:NOTES:END -->
