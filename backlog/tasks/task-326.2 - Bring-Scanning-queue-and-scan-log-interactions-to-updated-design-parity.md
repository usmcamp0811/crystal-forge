---
id: TASK-326.2
title: Bring Scanning and per-system CVE triage to updated design parity
status: Review
assignee:
  - Matt Camp
created_date: '2026-09-09 03:32'
updated_date: '2026-09-20 13:24'
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
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/329'
documentation:
  - docs/design/CrystalForge/components/ScanningView.jsx
  - docs/design/CrystalForge/components/CvesView.jsx
  - docs/design/CrystalForge/data-scanning.js
modified_files:
  - checks/web-ui/tests/integration-test.js
  - packages/default/.sqlx/query-43a8f1b05cacc24d800c9e8e393b1c5c666c277bb0.json
  - packages/default/.sqlx/query-90fc9f824b565ab238a995ad4a4c5a85dea79525ee.json
  - >-
    packages/default/crates/cf-server/migrations/0269_scanning_lifecycle_archive.sql
  - packages/default/crates/cf-server/src/api/models.rs
  - packages/default/crates/cf-server/src/bin/server.rs
  - packages/default/crates/cf-server/src/builder/cve_worker.rs
  - packages/default/crates/cf-server/src/handlers/api/cves.rs
  - packages/default/crates/cf-server/src/handlers/api/poam.rs
  - packages/default/crates/cf-server/src/handlers/api/scanning.rs
  - packages/default/crates/cf-server/src/handlers/api/systems.rs
  - packages/default/crates/cf-server/src/models/cve_scans.rs
  - packages/default/crates/cf-server/src/queries/cve_scan_diagnostics.rs
  - packages/default/crates/cf-server/src/queries/cve_scan_leases.rs
  - packages/default/crates/cf-server/src/queries/cve_scans.rs
  - packages/default/crates/cf-server/src/queries/scanning.rs
  - packages/default/crates/cf-server/src/queries/scanning_tests.rs
  - packages/default/crates/cf-server/src/services/cve_scans.rs
  - packages/default/crates/cf-server/src/services/poam.rs
  - packages/default/crates/cf-server/tests/poam_workflows.rs
  - packages/web-ui/assets/app.css
  - packages/web-ui/src/api/client.rs
  - packages/web-ui/src/api/models.rs
  - packages/web-ui/src/components/cve/mod.rs
  - packages/web-ui/src/components/cve/triage.rs
  - packages/web-ui/src/views/cves.rs
  - packages/web-ui/src/views/poam_api.rs
  - packages/web-ui/src/views/scanning.rs
  - packages/web-ui/src/views/system_detail.rs
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
- [x] #1 Active/Completed/BySystem tabs with correct state classification, selection semantics, and no activity side panel; Live indicator and Schedule button in header
- [x] #2 Active: scanning/queued/awaiting; Completed: terminal + history with archive/restore; BySystem: per-revision newest-first with superseded terminology
- [x] #3 Filters: text search, status, revision/freshness, latest-per-flake; sortable deterministic columns; visible result count; resettable empty state
- [x] #4 Detail tray: real status, trigger, scanner identity, findings, failure context, bounded log (no fake progress); log has search/download; running state shows elapsed time
- [x] #5 Failed stat card actionable (count > 0): click jumps to Completed tab, opens log; stale/failed rows show Check now/Retry/Build with exact deep links
- [x] #6 Selection guards: Active limits to scanning/queued (cancellation); Completed allows all (archive/restore); BySystem no selection
- [x] #7 Archive/restore on Completed: rows hidden not deleted, archived visually distinct, filtered counts honest, retention-caused empty states labeled
- [x] #8 System Detail CVE: unified triage modal (extracted from fleet view), environment-scoped, with Triage column and row action; no separate Justify/Create buttons
- [x] #9 Triage modal: outstanding/accepted/scheduled per environment; accepted needs justification; scheduled creates/reuses POA&M with owner/due/plan/milestones; typed assignee
- [x] #10 Authority: exact current evidence supports triage, legacy not promoted, missing/no-scan explicit, accepted/scheduled do NOT false-claim remediation/verification
- [x] #11 Browser assertions: Active/Completed switching, wait state rendering, failure actionability, archive/restore selection, log content, triage disposition, POA&M reuse, responsive/narrow/dark behavior
- [x] #12 Playwright: Active/Completed filtering, wait/failed transitions, log drawer, triage modal outstanding→accepted→scheduled, POA&M reuse, responsive tests; desktop/narrow/light/dark screenshots
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Finalize the in-progress Scanning slice: review the authoritative lifecycle/query contract, correct deterministic ordering and refresh/accessibility defects, add focused unit/query coverage, then rerun targeted server and Web UI checks and commit the slice.
2. Extract a shared CVE triage modal and typed state from the fleet CVE implementation. Reuse the existing TASK-440 evidence, locking, justification, and POA&M contracts instead of duplicating domain policy in the view.
3. Replace System Detail's separate Justify and Create POA&M actions with one environment-scoped triage action and Triage column. Represent outstanding, accepted, and scheduled independently for each environment; keep missing, no-scan, and legacy evidence explicit and never imply remediation or verification from disposition alone.
4. Wire accepted validation and scheduled POA&M create/reuse fields, including typed assignee, owner, due date, plan, and optional milestones, to the system-context triage API committed in `0913ea87`.
5. Update only the focused authoritative browser workflows `16c-scanning-view`, `12h-system-detail-cves-grouped-justification`, `12ha-system-detail-cve-inventory-fallbacks`, and `16-cves`. Cover wait/failed transitions, filtering, archive/restore, exact logs, triage transitions, POA&M reuse, responsive layouts, and light/dark screenshots.
6. Run proportional formatting, Rust tests/checks, SQLx/server checks, Web UI package build, and the single focused NixOS browser check. Inspect the final diff for documentation and scope, then commit, push, open an MR against `dev`, record evidence, and move the task to Review.

Constraints: safe cancellation is not implemented by the backend, so the UI MUST NOT expose a fake cancellation control. The user's host-preview waiver remains in force because TASK-440 owns the fixed ports; do not touch that worktree or its processes. Do not implement TASK-348.2 or run an unrestricted Web UI mega-check.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Started implementation in dedicated worktree `/home/mcamp/code/crystal-forge/TASK-326.2-scanning-cve-triage-parity` on branch `TASK-326.2-scanning-cve-triage-parity`, based exactly on `origin/dev` at `e304867c43abc8a7d8efe1f01e71af64cca67d8b`. The integration worktree contains unrelated untracked `session-ses_f927.md`; it does not overlap task scope and will not be modified.

BLOCKED before implementation by the user-defined stop condition for unrelated worktree overlap. `/home/mcamp/code/crystal-forge/TASK-440-system-config-flake-parity` has uncommitted changes in TASK-326.2 files: `packages/default/crates/cf-server/src/api/models.rs`, `handlers/api/scanning.rs`, `queries/scanning.rs`, `packages/web-ui/src/api/models.rs`, `views/scanning.rs`, and `views/system_detail.rs` (plus unrelated config explorer/flakes files and untracked generated CSS). No TASK-326.2 source edits or preview startup occurred. The dedicated task worktree remains clean at base `e304867c`. Awaiting user direction that preserves TASK-440 work before overlapping implementation proceeds.

User explicitly waived the other-worktree overlap stop condition and directed implementation to continue without touching TASK-440 leftovers. Re-fetch confirmed task HEAD and `origin/dev` both remain `e304867c43abc8a7d8efe1f01e71af64cca67d8b`; the task worktree is clean. Live preview startup is currently blocked because fixed ports 8080 and 3445 are owned by TASK-440 processes. Those processes will not be stopped or reused. Backend work can proceed while preview remains blocked; no browser-visible edit will be made without resolving the standing preview requirement or receiving an explicit exception.

Backend contract commit `0913ea87` adds migration 0269, durable `awaiting_build`/`awaiting_closure` states, automatic prerequisite promotion, immutable canonical trigger provenance, complete exact lifecycle history, bounded archive/restore metadata, richer exact details, and a System Detail environment-scoped triage API that reuses TASK-440 locking/evidence/POA&M semantics. Cancellation remains deliberately unsupported and all rows report `cancellable=false`; the future UI must not imply cancellation. Direct verification by the task owner: `nix develop -c cargo fmt --manifest-path packages/default/Cargo.toml --all -- --check` passed; `git diff --check` passed; `SQLX_OFFLINE=true nix develop -c cargo check --manifest-path packages/default/crates/cf-server/Cargo.toml --tests` passed with existing warnings; `nix run .#devScripts.cve-test -- up --tui=false` passed and shut down its isolated database. Preview remains blocked by the untouched TASK-440 fixed-port stack.

Host-side live preview was intentionally skipped because another worktree owned the legacy fixed development ports 8080/3445. The worktree was not disturbed. UI verification used focused compile/static checks followed by isolated authoritative NixOS browser workflows.

Scanning Web UI slice implemented without backend, CVE triage, or integration-test changes. The new view uses the authoritative scan-record collections and exact detail endpoint for Active, Completed, and By system. It adds real wait-count breakdowns, 15-second active/stat refresh, schedule GET/PUT handling, archive/restore, archived retention states, deterministic filters/sorts, per-system full-revision history, exact failed retry, actionable failed summary, and bounded diagnostic search/navigation/export with running-detail-only polling. No cancellation or fleet-wide rescan control is exposed because backend records are `cancellable=false`.

Modified files: `packages/web-ui/src/views/scanning.rs`, `packages/web-ui/src/api/models.rs`, `packages/web-ui/src/api/client.rs`, and `packages/web-ui/assets/app.css`. Backend commit `0913ea87` compiled against the client contract without requiring backend edits; no concrete backend contract defect was found.

Verification: `nix develop -c cargo test --manifest-path packages/web-ui/Cargo.toml` passed (429 passed, 1 ignored); `nix develop -c cargo check --manifest-path packages/web-ui/Cargo.toml --target wasm32-unknown-unknown` passed with existing warnings; targeted final `rustfmt --edition 2024 --check` passed for all three modified Rust files; `git diff --check` passed. Host preview and browser/integration workflows were intentionally not run under the user's explicit waiver and instruction not to touch processes or `integration-test.js`.

Closed the seven Scanning review gaps. The server now exposes nullable authoritative `started_at` from `lease_started_at` or `scan_metadata.execution_started_at`; the Web UI never derives running elapsed time from lifecycle creation. Completed badges/counts subtract hidden archives and distinguish filtered visible, loaded, available, and all rows. By-system history merges the per-system derivation projection with retained scan attempts so current/recent/superseded no-scan derivations render as `Never scanned` or `Needs build` with full revisions and no detail action; archive-state caches are keyed/invalidated by the selected view. Added stable tab IDs, ARIA panel labels, Arrow/Home/End tab navigation, and schedule-dialog initial focus, focus wrapping/restoration, and Escape precedence behind the detail drawer. Schedule copy now says `Superseded configs`; Active remains without selection or cancellation controls. Verification passed: server `SQLX_OFFLINE=true cargo check --tests`; Web UI tests 432 passed/1 ignored; WASM cargo check; targeted rustfmt check; `git diff --check`. Host preview and browser workflows remain waived per user instruction, and no ports/processes were touched.

Committed the finalized Scanning administration slice as `2c0d10e7`. Review fixes include queued terminology, lexicographic severity ordering, live-refresh-safe failed-stat opening, authoritative per-derivation relation labels in By system, and unique schedule-control accessible names. Verification after the fixes: focused Scanning tests passed (9/9), Web UI WASM cargo check passed with existing warnings, server offline cargo check passed with existing warnings, and `git diff --check` passed. Repository-wide Web UI `cargo fmt --check` remains blocked by pre-existing formatting differences in unrelated files; the changed Scanning file was formatted directly with rustfmt. Preview remains waived and TASK-440 processes were untouched.

Committed unified System Detail CVE triage as `1ca41260`. The Web UI now mirrors the system-context triage GET/POST contract, extracts shared fleet/system draft validation and scheduled metadata hydration, renders authoritative environment-scoped Outstanding/Accepted/Scheduled states after bounded expanded-package hydration, and replaces separate Justify/Create POA&M controls with one triage dialog. Scheduled reuse metadata is read-only, typed assignees preserve identity, default milestones apply only to new POA&Ms, inventory revision changes invalidate cached state, and async hydration/manual-open responses use per-row generation guards. Legacy/no-scan/conflict/whitelisted states remain explicit and non-actionable; accepted/scheduled copy does not claim remediation or verification. Owner verification: focused CVE tests passed (13/13), targeted rustfmt check passed, and `git diff --check` passed. The implementation agent also ran the full Web UI suite (436 passed, 1 ignored) and WASM cargo check successfully with existing warnings. Browser workflows remain outstanding.

Final verification completed. The authoritative focused NixOS browser check passed workflows `16c-scanning-view`, `12h-system-detail-cves-grouped-justification`, `12ha-system-detail-cve-inventory-fallbacks`, and `16-cves`, producing `/nix/store/6lnyyalkw4v52y9sni9a5dijkh91qcdb-vm-test-run-crystal-forge-web-ui-mega-integration` with 37 task-relevant screenshots. Representative desktop, narrow-desktop, tablet, light, and dark images were inspected. Static contracts, JavaScript syntax, targeted Rust formatting, `git diff --check`, Web UI tests/WASM checks, server offline checks, and server/Web UI documentation builds passed. Documentation builds retain unrelated existing warnings; the task-local private rustdoc link was corrected in `664a5bd8`. Host preview remained waived and TASK-440 ports/processes were not touched.

Branch `TASK-326.2-scanning-cve-triage-parity` was pushed at `664a5bd8`. MR creation is blocked because the configured GitLab CLI OAuth grant is expired or revoked (`invalid_grant`). Git push authentication remains valid. The task stays In Progress until an MR is open; use the GitLab branch link to create an MR targeting `dev`, or refresh `glab` authentication and resume finalization.

Opened GitLab MR !329 against `dev` at exact head `664a5bd88114b3bc2d47f7e49cc2f504b4d99f29`: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/329. The MR includes four representative screenshots from the authoritative browser result. GitLab pipeline 2920 started for the exact MR head and was still running when the task moved to Review.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
## Summary
- Adds durable Scanning lifecycle, wait-state, archive/restore, trigger, diagnostic, and per-system history contracts.
- Rebuilds the Scanning administration view around Active, Completed, and By system workflows with deterministic filtering, sorting, exact details, retention-aware counts, schedule controls, and accessible responsive interactions.
- Extracts shared CVE triage behavior and adds environment-scoped Outstanding, Accepted, and Scheduled decisions to System Detail while preserving exact-evidence authority and POA&M reuse rules.
- Adds focused authoritative browser coverage for Scanning, System Detail CVE triage and inventory fallbacks, and fleet CVE triage.

## Verification
- Web UI Rust tests and WASM checks passed.
- Server offline checks and focused database-backed CVE tests passed.
- JavaScript syntax and static UI contracts passed.
- Focused NixOS browser workflows passed and produced 37 inspected task screenshots.
- Server and Web UI documentation builds passed with unrelated existing warnings.
- Targeted rustfmt checks and `git diff --check` passed.

## Risk and compatibility
- Scan cancellation remains unsupported and the UI does not expose a false cancellation control.
- Accepted and Scheduled dispositions do not claim remediation or verification. Closure still requires later exact scan evidence.
- Host preview was explicitly waived to preserve the TASK-440 fixed-port stack; isolated NixOS browser verification supplied the authoritative UI evidence.
<!-- SECTION:FINAL_SUMMARY:END -->
