---
id: TASK-433.7
title: >-
  TASK-433 Phase 6: POA&M integration in evidence/finding, system, bundle,
  assignment UI
status: In Progress
assignee:
  - '@opencode-agent'
created_date: '2026-08-23 01:43'
updated_date: '2026-08-27 17:45'
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

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
## Phase 6 implementation plan

1. Add the explicitly authorized minimal Phase-5 contract correction, preserving server authority and authorization: bounded batch finding-remediation lookup by authoritative assessment IDs (active plus completed history), server-filtered compatible POA&M search from an authoritative failing assessment, and bounded assignment-version relationship lookup. Reuse existing visibility, compatibility, one-active-remediation, list/detail summary, and pagination logic; add focused HTTP/DB tests and record the correction on TASK-433.6.
2. Add one typed Web UI POA&M API module mirroring Phase-5 DTOs and error envelopes. Centralize authenticated GET and CSRF mutations, revisions, 403/404/409/412 mapping, bounded batch chunking (100 IDs), stale-response suppression, and authoritative-response reconciliation.
3. Build a shared route/state-driven POA&M detail tray used by compliance, system, bundle, and assignment surfaces. Include metadata, lifecycle, verification/closure/reopen, findings, exact evidence navigation, assignment references, plan, milestones, activity/notes, and deliberate loading/not-visible/error/conflict states.
4. Integrate failed evidence rows with a separate remediation dimension: active and historical POA&M lookup, real-context Create, server-compatible Link Existing, exact stable-ID navigation, and explicit copy/tests proving FAIL remains FAIL and waiver actions remain separate.
5. Integrate System Detail compliance with committed server rollup counts and Open/Overdue/Closed/All filters, compact real POA&M rows, common detail, and exact linked-finding navigation. Use batched system rollups for visible lists rather than per-row calls.
6. Integrate bundle compliance with authoritative open/on-POA&M/no-POA&M/overdue/awaiting-verification/completed rollups and filtered real rows. Chunk visible bundle IDs by 100 and add request-count instrumentation proving no N+1.
7. Integrate immutable assignment-version relationships in the existing assignment surface using the separate relationship API only. Display and open referenced POA&Ms; link/unlink against exact assignment version/revision; prove assignment identity/digest/config and current-version pointer remain unchanged.
8. Add pure DTO/presentation/navigation/error tests plus real server-backed browser workflows for finding create/link, detail edits/milestones/activity, closure rejection while FAIL, system integration, bundle rollups/no-N+1, and assignment link/unlink immutability. Extend the coverage manifest and critical Web UI checks; capture supported screenshots.
9. Run exact TASK-433.7 verification, focused Phase-5 contract tests, syntax/diff checks, then perform independent requirements/API-authority/UI-state/E2E/performance/regression/design-comparison reviews. Resolve all P0/P1/P2.
10. Only after AC1-AC4 are objectively proven: check exactly four ACs, move TASK-433.7 to Review, update TASK-433.6 correction evidence and MR !318, commit/push the rebased branch, confirm clean local/remote SHA and CI, then stop before Phase 7.

Interrupted-agent recovery: inspect the full worktree diff, move assignment relationship/link UI into `AssignmentListPanel` with its existing local signals and generation guards, normalize new Dioxus data attributes, fix only compile-required `EvidenceDrawer` call interfaces, then run fmt/check/tests and focused prohibited-global scans before reporting without committing.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Preflight: rebased `TASK-433-policy-poam-workflows` onto current `origin/dev` (`79153f74`) without discarding later legitimate changes. Replayed Phase 1-5 implementation; Phase-5 commit is now `42488e89` (rebased equivalent of accepted `68904343`) and remains the branch head before Phase 6. TASK-433.6 is Review with all six ACs checked and successful exact-head pipeline 2794857147 recorded. Dedicated worktree is `/home/mcamp/code/crystal-forge/TASK-433-policy-poam-workflows`; scope is exactly AC1-AC4, with Phase 7 dashboard/notifications/coach explicitly excluded.

Phase-6 contract audit found blocking Phase-5 API omissions before any implementation write. AC1 cannot be implemented without client-side approximation/N+1 because evidence DTOs expose only an assessment ID and no stable finding/remediation relationship, while Phase 5 has no batch active/historical remediation lookup for visible findings/assessment IDs. AC1 Link Existing is also directionally unsupported: `GET /poams/:poam_id/compatible` returns findings compatible with a known POA&M, but the required finding-origin flow needs server-filtered POA&Ms compatible with a supplied real finding/assessment; broad `/poams?policy_lineage_id=...` listing would shift compatibility/active-link decisions into the browser. AC4 cannot render first-class relationships from the assignment surface because there is no assignment-version filter or batch lookup; assignment references are discoverable only after fetching an already-known POA&M detail. These gaps belong to TASK-433.6 AC2 and its explicit Phase-5 scope for authenticated compatible search and batched active links/assignment relationships. Per the user's stop rule, no Phase-6 code or tests were written. Required Phase-5 correction would expose bounded authorization-scoped contracts for (1) active plus completed POA&M relationships by assessment/finding IDs, (2) compatible POA&M search by authoritative assessment/finding context, and (3) POA&M relationships by immutable assignment-version IDs. Awaiting explicit authorization to amend Phase 5 or another server-contract decision.
<!-- SECTION:NOTES:END -->
