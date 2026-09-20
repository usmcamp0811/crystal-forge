---
id: TASK-326.2
title: Bring Scanning and per-system CVE triage to updated design parity
status: In Progress
assignee:
  - Matt Camp
created_date: '2026-09-09 03:32'
updated_date: '2026-09-20 04:09'
labels:
  - scanning
  - web-ui
  - design-parity
  - cve-triage
dependencies:
  - TASK-326.1
  - TASK-337
references:
  - e79d0ad6
  - ad6589e1
  - docs/design/CrystalForge/components/ScanningView.jsx
  - docs/design/CrystalForge/components/CvesView.jsx
  - docs/design/CrystalForge/components/SystemDetail.jsx
documentation:
  - docs/design/CrystalForge/components/ScanningView.jsx
  - docs/design/CrystalForge/components/CvesView.jsx
  - docs/design/CrystalForge/data-scanning.js
modified_files:
  - packages/web-ui/src/views/scanning.rs
  - packages/web-ui/src/views/system_detail.rs
  - packages/web-ui/src/components/cve/mod.rs
  - packages/web-ui/src/api/models.rs
  - packages/web-ui/assets/app.css
  - checks/web-ui/tests/integration-test.js
parent_task_id: TASK-326
priority: high
type: enhancement
ordinal: 473000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Rewrite the production Scanning view to align with e79d0ad6 (Active/Completed/BySystem tabs with wait states, archive/restore, failure diagnostics) and add unified CVE triage modal to System Detail per ad6589e1 (environment-scoped outstanding/accepted/scheduled with POA&M create/reuse, typed assignee).

## Scanning: Active vs Completed (not Deployed/All)

**Active tab**: scanning/queued/awaiting states; "awaiting" = blocked on build/cache (not failure). Selection limits to cancellable rows (scanning/queued). Stats show "scanning now" with breakdown: scanning · queued · awaiting closure.

**Completed tab**: terminal results + history. Archive/restore without deletion; archived rows hidden and visually distinct. Selection for history ops (archive/restore), not cancellation. Filtered empty views caused by retention say so, not "no scans exist".

**BySystem**: per-revision history, newest-first, with superseded config terminology; hidden count + link; unscanned/needs-build explicit.

**Controls**: Live indicator, Schedule button, no global Rescan All (check if rescan actions move elsewhere).

**Filtering/Sorting**: text search, status, revision/freshness, latest-per-flake; deterministic sortable columns (status/severity/revision/timestamp); visible result count; resettable empty state.

**Detail tray**: real status, trigger, scanner identity, findings, failure context, bounded log content (no fake progress).

**Failed actionability**: stat card click jumps to Completed, opens failing scan's log.

**By-system history**: exact per-revision scans, needs-build/never-scanned explicit, hidden older scans show count.

**Wait states**: explicit "awaiting: <reason>"; stale/failed provide Check now/Retry/Build with exact deep links.

## System Detail CVE: Unified Triage Modal

Replace separate Justify + Create POA&M with one CveTriageModal (extracted from fleet CVE) scoped to current system's environment.

**Environment-scoped decision**: each environment gets outstanding/accepted/scheduled choice. Accepted requires justification + optional review date. Scheduled creates/reuses POA&M with owner, due, plan, optional milestones.

**Presentation**: add Triage column (outstanding/accepted/scheduled state); row action opens modal; no separate Justify/Create buttons.

**Authority (CRITICAL)**: preserve TASK-440 rules—exact current evidence can support triage, legacy not promoted, missing/no-scan explicit, accepted/scheduled do NOT false-claim remediation/verification, only exact evidence/verification supports closure.

## Dependencies & Overlap

- TASK-326.1 (wait states, logs) & TASK-337 (trigger) are blockers for detail/log implementation
- TASK-348.2 superseded by this task; make canonical and note supersession
- Reuse existing TASK-440 CVE triage domain/client; preserve authority constraints
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Active/Completed/BySystem tabs with correct state classification, selection semantics, and no activity side panel; Live indicator and Schedule button in header
- [ ] #2 Active: scanning/queued/awaiting; Completed: terminal + history with archive/restore; BySystem: per-revision newest-first with superseded terminology
- [ ] #3 Filters: text search, status, revision/freshness, latest-per-flake; sortable deterministic columns; visible result count; resettable empty state
- [ ] #4 Detail tray: real status, trigger, scanner identity, findings, failure context, bounded log (no fake progress); log has search/download; running state shows elapsed time
- [ ] #5 Failed stat card actionable (count > 0): click jumps to Completed tab, opens log; stale/failed rows show Check now/Retry/Build with exact deep links
- [ ] #6 Selection guards: Active limits to scanning/queued (cancellation); Completed allows all (archive/restore); BySystem no selection
- [ ] #7 Archive/restore on Completed: rows hidden not deleted, archived visually distinct, filtered counts honest, retention-caused empty states labeled
- [ ] #8 System Detail CVE: unified triage modal (extracted from fleet view), environment-scoped, with Triage column and row action; no separate Justify/Create buttons
- [ ] #9 Triage modal: outstanding/accepted/scheduled per environment; accepted needs justification; scheduled creates/reuses POA&M with owner/due/plan/milestones; typed assignee
- [ ] #10 Authority: exact current evidence supports triage, legacy not promoted, missing/no-scan explicit, accepted/scheduled do NOT false-claim remediation/verification
- [ ] #11 Browser assertions: Active/Completed switching, wait state rendering, failure actionability, archive/restore selection, log content, triage disposition, POA&M reuse, responsive/narrow/dark behavior
- [ ] #12 Playwright: Active/Completed filtering, wait/failed transitions, log drawer, triage modal outstanding→accepted→scheduled, POA&M reuse, responsive tests; desktop/narrow/light/dark screenshots
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Preserve current exact-evidence and execution-fencing foundations. Port only the missing TASK-337 trigger attribution into current `source_trigger` semantics, including immediate/local post-build/periodic paths and unknown-value round trips.
2. Add migration 0269 with additive scan lifecycle and archive metadata. Represent build and closure waits as non-terminal active states, include them in active deduplication, and preserve immutable trigger/evidence/diagnostic records. Add bounded idempotent terminal archive/restore and cancellation only where execution ownership can enforce it.
3. Extend scanning queries and admin APIs with authoritative Active/Completed/history projections, stable totals and ordering, exact detail metadata, waiting/failure context, deterministic failed identity, and server-owned archive visibility. Keep diagnostics bounded and redacted.
4. Update the worker and exact trigger flows so one logical waiting scan transitions automatically when its exact prerequisite becomes available, without weakening leases, execution tokens, stale recovery, or duplicate-active-scan protection.
5. Refactor TASK-440 CVE triage into shared service and Dioxus presentation code. Add a system-context endpoint that derives the current environment and exact subject set server-side, clearly applies the decision to all exact affected hosts in that environment, preserves canonical lock ordering and evidence revalidation, and never promotes legacy inventory or ordinary justification.
6. Replace Scanning with Active/Completed/By system, truthful selection/actions, filters/sorts/counts, archive/restore, actionable failure, exact detail/log search/download, schedule behavior, accessibility, and responsive theme styling. Replace System Detail Justify/Create POA&M actions with the shared authority-aware triage experience while retaining backward-compatible legacy justification data.
7. Extend focused Rust/PostgreSQL/API coverage and evolve workflows 16c, 12h, 12ha, and 16-cves. Run static checks, isolated database/SQLx preparation, WASM compile, host-compatible browser workflows, then the single selected authoritative VM run.
8. Record objective AC #1–#12 evidence, document TASK-337 incorporation and the precise TASK-326.1 audit result, commit in reviewable units, push, open an MR to `dev`, and move TASK-326.2 to Review only after all required evidence passes.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Started implementation in dedicated worktree `/home/mcamp/code/crystal-forge/TASK-326.2-scanning-cve-triage-parity` on branch `TASK-326.2-scanning-cve-triage-parity`, based exactly on `origin/dev` at `e304867c43abc8a7d8efe1f01e71af64cca67d8b`. The integration worktree contains unrelated untracked `session-ses_f927.md`; it does not overlap task scope and will not be modified.

BLOCKED before implementation by the user-defined stop condition for unrelated worktree overlap. `/home/mcamp/code/crystal-forge/TASK-440-system-config-flake-parity` has uncommitted changes in TASK-326.2 files: `packages/default/crates/cf-server/src/api/models.rs`, `handlers/api/scanning.rs`, `queries/scanning.rs`, `packages/web-ui/src/api/models.rs`, `views/scanning.rs`, and `views/system_detail.rs` (plus unrelated config explorer/flakes files and untracked generated CSS). No TASK-326.2 source edits or preview startup occurred. The dedicated task worktree remains clean at base `e304867c`. Awaiting user direction that preserves TASK-440 work before overlapping implementation proceeds.

User explicitly waived the other-worktree overlap stop condition and directed implementation to continue without touching TASK-440 leftovers. Re-fetch confirmed task HEAD and `origin/dev` both remain `e304867c43abc8a7d8efe1f01e71af64cca67d8b`; the task worktree is clean. Live preview startup is currently blocked because fixed ports 8080 and 3445 are owned by TASK-440 processes. Those processes will not be stopped or reused. Backend work can proceed while preview remains blocked; no browser-visible edit will be made without resolving the standing preview requirement or receiving an explicit exception.

Backend contract commit `0913ea87` adds migration 0269, durable `awaiting_build`/`awaiting_closure` states, automatic prerequisite promotion, immutable canonical trigger provenance, complete exact lifecycle history, bounded archive/restore metadata, richer exact details, and a System Detail environment-scoped triage API that reuses TASK-440 locking/evidence/POA&M semantics. Cancellation remains deliberately unsupported and all rows report `cancellable=false`; the future UI must not imply cancellation. Direct verification by the task owner: `nix develop -c cargo fmt --manifest-path packages/default/Cargo.toml --all -- --check` passed; `git diff --check` passed; `SQLX_OFFLINE=true nix develop -c cargo check --manifest-path packages/default/crates/cf-server/Cargo.toml --tests` passed with existing warnings; `nix run .#devScripts.cve-test -- up --tui=false` passed and shut down its isolated database. Preview remains blocked by the untouched TASK-440 fixed-port stack.

Host-side live preview was intentionally skipped because another worktree owned the legacy fixed development ports 8080/3445. The worktree was not disturbed. UI verification used focused compile/static checks followed by isolated authoritative NixOS browser workflows.
<!-- SECTION:NOTES:END -->
