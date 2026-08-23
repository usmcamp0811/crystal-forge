---
id: TASK-433.6
title: 'TASK-433 Phase 5: POA&M database schema, API, auth/audit, and server tests'
status: Backlog
assignee: []
created_date: '2026-08-23 01:43'
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
- [ ] #1 Normalized POAM tables, links, milestones, activity/history and verification references exist through additive migrations with constraints/indexes.
- [ ] #2 Authenticated APIs implement POAM creation, detail/list/filter/search, update, transitions, milestones, notes, links, verification, close, reopen, system/bundle rollups and dashboard sources.
- [ ] #3 Server validates finding context, compatibility, active-link invariant if applicable, authorization/CSRF and stale/conflict conditions.
- [ ] #4 POAM creation/linking never changes the underlying evaluation result; FAIL remains FAIL.
- [ ] #5 Closure is authoritative and race-safe, requires current Pass or documented accepted waiver for all linked findings, stores verification and rejects failing/error/unknown/not-checked/stale findings.
- [ ] #6 POAM server tests cover real finding creation, multi-finding links, invalid links, active invariant, milestones, activity, transitions, overdue, closure rejection/acceptance, verification storage, reopen, filters, auth and concurrency.
<!-- AC:END -->
