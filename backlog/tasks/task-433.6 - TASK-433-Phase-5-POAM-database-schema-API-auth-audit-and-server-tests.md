---
id: TASK-433.6
title: 'TASK-433 Phase 5: POA&M database schema, API, auth/audit, and server tests'
status: Review
assignee:
  - '@opencode-agent'
created_date: '2026-08-23 01:43'
updated_date: '2026-08-28 05:20'
labels:
  - design-parity
  - poam
  - server
  - database
  - phase-5
dependencies:
  - TASK-433.5
references:
  - TASK-433
  - TASK-433.1
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/318'
documentation:
  - docs/design/CrystalForge/components/PoamViews.jsx
  - docs/design/CrystalForge/data-poam.js
  - docs/agent/database-safety.md
parent_task_id: TASK-433
priority: high
type: feature
ordinal: 438000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Objective
Parent umbrella: TASK-433, phase 5 of 8 (contextual only). Builds the entirely new POA&M subsystem foundation: normalized database schema (additive migrations only), authenticated server APIs, authorization/audit wiring, and server-side test coverage. No production POA&M persistence/API exists today (confirmed in TASK-433.1).

## Explicit scope
- Normalized POA&M tables: stable DB/human IDs, title/plan/owner/target/risk/creator/timestamps/closure reference, statuses `open`, `in_progress`, `blocked`, `awaiting_verification`, `completed`, stable finding links, milestones (server-dated default offsets 14/28/35/49/56 days), activity/history, closure verification. Enforce one active remediation per finding if retained; assignment references never mutate immutable versions.
- New additive migrations only, current FK/type/index conventions, valid-link/invariant constraints, indexes for status/date/human ID/system/policy/bundle/requirement/active links/activity. SQLx metadata refreshed against isolated local PostgreSQL per docs/agent/database-safety.md.
- Authenticated APIs: create-from-real-finding, detail/list/filter/search, updates/status transitions, milestones, notes/history, link/unlink, compatible search, verify/close/reopen, system/bundle rollups, dashboard summary/watchlist sources.
- Server validates finding context, compatibility (not title), active-link invariant, authorization/CSRF, and stale/conflict conditions using existing session/audit conventions.
- POA&M creation/linking never changes the underlying evaluation result; FAIL remains FAIL.
- Closure is transactional and race-safe: rechecks linked results, requires current Pass or documented accepted waiver for every linked finding, stores verification, and rejects Fail/Error/Unknown/NotChecked/stale findings.
- Batch queries: no POA&M query per system/policy/finding; batch active links, rollups, dashboard/detail sources.
- Audit create, field changes, status, milestones, notes, links, verify, close, reopen, and assignment relationships using existing audit patterns.
- Server-side tests: real finding creation, multi-finding links, invalid links, active invariant, milestones, activity, transitions, overdue derivation, closure rejection/acceptance, verification storage, reopen, filters, auth, and concurrency.

## Explicit non-scope
No UI work in this subtask (Phase 6/7 consume this API). Do not turn POA&M into a waiver/Pass mechanism. No fixtures/seeded POAMs/synthetic data.

## Verification
```bash
nix develop -c cargo fmt --manifest-path packages/default/Cargo.toml -- --check
nix develop -c cargo test --manifest-path packages/default/Cargo.toml
nix build .#packages.x86_64-linux.server --no-link
nix develop -c bash -c 'cd packages/default && cargo sqlx prepare --workspace'
nix build .#checks.x86_64-linux.server-regressions --no-link
nix build .#checks.x86_64-linux.integration --no-link
```
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Normalized POAM tables, links, milestones, activity/history and verification references exist through additive migrations with constraints/indexes.
- [x] #2 Authenticated APIs implement POAM creation, detail/list/filter/search, update, transitions, milestones, notes, links, verification, close, reopen, system/bundle rollups and dashboard sources.
- [x] #3 Server validates finding context, compatibility, active-link invariant if applicable, authorization/CSRF and stale/conflict conditions.
- [x] #4 POAM creation/linking never changes the underlying evaluation result; FAIL remains FAIL.
- [x] #5 Closure is authoritative and race-safe, requires current Pass or documented accepted waiver for all linked findings, stores verification and rejects failing/error/unknown/not-checked/stale findings.
- [x] #6 POAM server tests cover real finding creation, multi-finding links, invalid links, active invariant, milestones, activity, transitions, overdue, closure rejection/acceptance, verification storage, reopen, filters, auth and concurrency.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
## Phase 5 implementation plan

1. Introduce a stable persisted finding lineage keyed by `(system_id, policy_lineage_id)`; keep exact policy version, effective-set/config digest, derivation/store path, assessment/result and requirement/bundle/assignment context as observation or verification references rather than finding identity.
2. Add additive migrations after current 0233 for normalized POA&M core, sequence-backed human IDs, active/historical finding links, immutable activity, stable milestones, assignment-version references, verification attempts/items, and the minimal generic accepted finding-waiver foundation authorized by the user. Add DB constraints, composite FKs, partial uniqueness for one active remediation per finding, and indexes matching actual filters/rollups.
3. Add typed POA&M domain/DTO/query/service modules and canonical batch current-finding resolution. Keep evaluation/composite rows read-only: creation, linking, lifecycle, milestones, notes, verification attempts and unlinking must never mutate compliance outcomes.
4. Implement authenticated REST APIs for create/detail/list/filter/search/update, explicit transitions, milestone lifecycle, immutable notes, finding/assignment link management, compatible search, verify/close/reopen, batched system/bundle rollups, dashboard summary and watchlist. Use bounded pagination and typed machine-readable errors.
5. Enforce existing session/role, environment membership/system visibility and CSRF conventions. Hide a multi-finding POA&M unless every linked context is visible; require mutation access to every affected context. Use transactional POA&M activity plus existing admin audit rows.
6. Use POA&M revisions on every lifecycle-sensitive mutation. Enforce compatibility by shared policy lineage (same canonical control across systems), never title. Require at least one real finding; assignment references are supplemental and never mutate immutable assignment versions.
7. Implement closure/reopen as serializable, retryable transactions with deterministic finding locks, POA&M/link/result rechecks, exact structural freshness tokens, current Pass or exact accepted non-expired/non-revoked finding waiver per link, immutable verification attempts/items, active-link retirement/reactivation and atomic activity/audit.
8. Add real migrated-DB/API/concurrency regressions for AC1-AC6, including human-ID and active-link races, closure versus superseding Fail, waiver applicability lifecycle, FAIL byte/semantic preservation, assignment immutability, overdue boundaries, all filters/rollups/dashboard sources, auth/CSRF and audit payloads.
9. Refresh SQLx metadata only against the repository isolated PostgreSQL workflow. Run all six required verification commands plus focused tests and `git diff --check`.
10. Stop coding for independent requirements, integrity, authorization, closure, N+1/error and regression review. Resolve all P0/P1/P2, then check exactly AC1-AC6, move TASK-433.6 to Review, update MR !318, commit/push and stop before Phase 6.

Independent-review remediation checkpoint: move canonical effective-policy resolution into the serializable closure transaction; acquire deterministic finding locks before every scan/deployment rule-result mutation; decouple immutable verification snapshots from mutable assessment/derivation rows and preserve Phase-4 cleanup semantics; bind accepted waivers to exact Fail observations and expire/uniqueness-map them transactionally.

Complete all-context visibility and attribution for assignment references; distinguish active, explicitly unlinked, and closure-retired finding links; use authoritative exact current/closure context for detail/search/filter/system/bundle rollups; expose complete bounded verification/history and assignment/bundle/requirement navigation DTOs; validate and bound list/batch inputs with typed errors.

Make activity/audit history truthful and reconstructable, enforce completed/active-link invariants, return revisions on rejected closure, make duplicate links idempotent or typed conflicts, improve query/index plans, and add forced-concurrency plus exact semantic regressions for every review finding before rerunning independent review and required verification.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Phase-5 lock/preflight: implementation is active in dedicated worktree `/home/mcamp/code/crystal-forge/TASK-433-policy-poam-workflows` on branch `TASK-433-policy-poam-workflows`, based on successful Phase-4 exact head `b238ff9e969b525aa44d46ef27beb4faefc30e12`. Pipeline 2791067463 is green and MR !318 is conflict-free. Scope is server/database POA&M foundation only; Phase 6 UI and Phase 7 consumers are not started. Acceptance criteria remain unchecked until full implementation, verification, and independent review pass.

Implemented Phase 5 in dedicated worktree and pushed commit `68904343` to MR !318. Final closure/verification transactions acquire deterministic finding advisory locks before authoritative reads and intentionally use READ COMMITTED so statements after lock waits observe the committing writer; forced races cover superseding assessments, direct applicability, aggregate rule results, and waiver revocation. Added database-enforced immutable finding/link/evidence identity, sealed verification attempts, exact Pass/accepted-waiver closure constraints, all-history environment visibility, non-oracular authorization, bounded batch expansion, and keyset pagination for mutable history feeds.

Verification completed against repository-isolated PostgreSQL on port 3042: fresh migration chain through 0234 and `cargo sqlx prepare --workspace` passed with no metadata delta; focused POA&M suite passed 13/13; full workspace Rust tests passed 1221 with 0 failures and 386 ignored; server package, server-regressions, and integration Nix builds passed; formatting and `git diff --check` passed. Final independent integrity, authorization, and correctness reviews found no remaining P0/P1/P2 issues.

Exact-head GitLab pipeline 2794857147 passed for commit `68904343a488e6a5c909fa80c4bac16052814ff1`: https://gitlab.com/crystal-forge/crystal-forge/-/pipelines/2794857147. MR !318 is conflict-free and remains open for review. Per repository lifecycle, the task remains Review until the MR is merged.

Phase-6 authorized contract correction completed on 2026-08-28: added bounded authenticated finding-remediation relationships by authoritative assessment IDs, server-filtered compatible POA&M search from a current Fail assessment, and immutable assignment-version relationship lookup. Assignment compatibility now pairs each scope with the same finding's policy lineage rather than independently matching scope and lineage across findings. PostgreSQL-backed `poam_workflows` verification passed 16/16, including the new cross-finding scope/lineage regression; Phase-6 browser workflows 29g–29m and the full Web UI Nix check also passed. These are minimal Phase-5 API corrections required by TASK-433.7 and remain part of MR !318.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
## Summary
- Added the normalized POA&M persistence model, stable finding lineage, immutable historical links/activity/verification evidence, milestones, assignment-version references, exact finding waivers, and database-enforced closure invariants.
- Added authenticated and CSRF-protected server APIs for creation, detail/list/search/filter, updates/transitions, milestones/notes, finding and assignment relationships, verification, close/reopen, compatible findings, dashboard/watchlist, and system/bundle rollups.
- Integrated canonical effective-policy resolution and deterministic finding locks with assessment/CVE/applicability writers so closure rechecks exact current Pass or accepted unexpired waiver evidence without changing compliance outcomes.
- Added bounded keyset history pagination, hidden-context authorization protections, typed errors, resource limits, and migrated database/API/concurrency regressions.

## Verification
- `cargo fmt --all --check`
- fresh isolated migration chain and `cargo sqlx prepare --workspace` (no metadata delta)
- full Rust workspace: 1221 passed, 0 failed, 386 ignored
- focused POA&M workflows: 13 passed, 0 failed
- server package Nix build
- server-regressions Nix check
- integration Nix check
- final independent integrity, authorization, and correctness reviews: no remaining P0/P1/P2 findings

Pushed as `68904343` to MR !318. Phase 6 UI work remains out of scope.
<!-- SECTION:FINAL_SUMMARY:END -->
