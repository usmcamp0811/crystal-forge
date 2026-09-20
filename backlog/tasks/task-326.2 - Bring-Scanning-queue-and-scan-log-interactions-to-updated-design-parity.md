---
id: TASK-326.2
title: Bring Scanning and per-system CVE triage to updated design parity
status: In Progress
assignee:
  - Matt Camp
created_date: '2026-09-09 03:32'
updated_date: '2026-09-20 17:44'
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

Focused MR follow-up: implement design commit `35519546` for System Detail CVE triage with default host scope and optional server-derived environment scope. Add append-only host disposition history, host-over-environment effective precedence, scope-safe System Detail DTOs, exact-subject POA&M behavior, inherited row chips, truthful fixed-version wording, focused PostgreSQL/frontend/browser regressions, then push the existing MR and return the task to Review.

Focused backend follow-up: add migration 0270 with append-only host disposition history and active identity uniqueness; extend the System Detail API with a server-derived host/environment scope enum and explicit direct/inherited/effective state; split host and environment mutations while preserving canonical CVE -> system sentinel -> policy -> exact finding lock order, exact evidence, compatible POA&M reuse, and typed conflicts; make close/reopen restore scheduled dispositions in their recorded original scope; update focused PostgreSQL regressions and run scoped rustfmt, database tests, SQLX_OFFLINE server check, and git diff --check. Only cf-server backend files and genuinely changed SQLx metadata are in scope.

Focused frontend follow-up: mirror the host/environment System Detail triage DTO contract in `poam_api.rs`; make `CveTriageDraft` hydrate one selected scope and regenerate complete scope-specific defaults on every switch while preserving fleet request serialization; update the System Detail dialog and row state presentation for host precedence, inherited environment state, truthful fix wording, and existing accessibility/concurrency gates; add focused Rust regressions and run touched-file rustfmt, focused Web UI tests, WASM check when practical, and `git diff --check`.

Backend acceptance corrections: derive environment POA&M subject sets and coherence from exact hosts without active host overrides; make host ACCEPTED retire inherited environment ownership for only the selected exact link; make host OPEN restore that link only when the inherited environment POA&M remains coherent for all other inherited subjects; preserve typed conflicts for incompatible active owners. Narrow migration 0270's detached-host exception to one historical exact subject and no environment-disposition history. Add active-link ownership and migration-invariant regressions, then rerun serial poam_workflows, touched-file rustfmt, offline cargo check, and diff hygiene.

Review follow-up at `7c4c27d3`: add a private filesystem-identity helper in both agent and legacy server collectors. Preserve canonical root equality. For different resolved roots, require all stable NixOS sentinels (`init`, `system`, and `bin/switch-to-configuration`) to resolve to identical targets; unresolved roots return unknown and names alone never establish identity. Add isolated filesystem tests including the production-shaped `system-3079-link` wrapper.

Extend the existing PostgreSQL inventory regression to prove the strict authority sequence Exact -> RetainedGenerationUnavailable -> CurrentStoreMismatch while keeping the latest-state-first query unchanged. Treat the newly discovered migration-0259 stale-state defect as a separate schema/security scope decision; do not silently add a migration in this follow-up.

Replace only user-visible CVE fallback terminology with Historical wording. Preserve backend wire variants, `legacy_*` fields, test fixture identifiers, and read-only authority behavior. Update focused browser/static text assertions, then run focused Rust/UI/static checks before deciding whether one final authoritative browser run is required.

Scanning workflow parity repair from current pushed head `19352160`: centralize `on_build` scan-intent creation in one transaction-local query helper. Every production build-job insertion path calls it only for rows actually inserted. The helper gates on persisted `on_build`, accepts only NixOS derivations, inserts immutable `post_build` provenance in `awaiting_build`, and uses the existing active partial uniqueness constraint with conflict-do-nothing so manual/fleet provenance wins.

Adjust successful build completion to advance an existing `awaiting_build/post_build` intent through the existing prerequisite rules without creating another state machine. Keep build failure as a truthful waiting prerequisite so Vulnix is never blamed or claimed; retries and restarts reuse the durable active row. Preserve remote closure gating and add prompt calls to the authoritative bounded promotion helper where completion/publication events already have a safe post-commit hook.

Reshape the production Scanning drawer around configuration identity, a concise real lifecycle strip, typed wait/failure callouts, and accessible Log/Details tabs. Keep bounded persisted diagnostics, redaction, search/navigation/highlighting/export, stale-request fencing, and all advanced metadata. Render diagnostics as terminal-style lines. Add only real retry and exact build navigation actions backed by server-issued identity.

Expand focused PostgreSQL coverage across enabled/disabled policy, idempotent and concurrent insertion, local/remote promotion, non-claimable waits, build failure/retry/restart persistence, existing manual/fleet provenance, and completed-evidence semantics. Update only the 16c browser workflow for failed, waiting, running, completed, responsive, focus, Log/Details, and focused screenshots. Run cheap gates first, then exactly one final 16c VM command at candidate HEAD.

Backend post-build intent follow-up: add one transaction-local helper that inserts `awaiting_build` post-build intent only for supplied NixOS derivations when persisted `on_build` is enabled, using the active partial conflict target and `DO NOTHING`. Make all four authoritative build admission functions use exact `RETURNING` derivation IDs and call the helper in the same transaction only for inserted jobs. Reshape successful build completion to attach the completed job and transition the existing post-build intent to `pending` for local output or `awaiting_closure` for remote output without cache publication; retain policy/type-gated repair for legacy missing intent and never mutate manual/fleet provenance. Add focused ignored PostgreSQL regressions for admission gates, idempotency/provenance, completion states, non-claimable waits, failures, and retry/replacement behavior. Run targeted rustfmt and the smallest dedicated PostgreSQL test selection.
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

Resumed TASK-326.2 in its existing dedicated worktree and MR !329. Fast-forwarded the clean task branch to the intentional design-only head `35519546fc5bb584922643b498cf722429f185d5`. The follow-up is limited to System Detail CVE host/environment triage, shared truthful fixed-version wording, and focused regression coverage. The existing host-preview waiver remains active; no other worktree or preview process will be touched. Migration inspection shows 0269 is the current highest migration, so the additive host-disposition migration will use 0270.

Focused backend follow-up implemented and locally verified without commit or push. Added migration 0270 append-only host CVE dispositions with one-active identity uniqueness and the documented host-only orphan POA&M exception. System Detail POST accepts only `scope: host|environment`; GET now returns selected hostname, derived environment identity/name, exact environment subjects/count, direct host/environment dispositions, effective disposition/source, and scheduled POA&M metadata. Host mutations preserve canonical CVE -> system sentinel -> policy -> exact-finding lock order, exact current evidence, typed conflicts, compatible POA&M reuse/host-only expansion, independent OPEN behavior, and host-over-environment precedence. Close/reopen restores scheduled dispositions in their recorded original host or environment scope. Added direct regressions for the complete mixed-state precedence matrix, active identity uniqueness, independent scope opening, compatible/incompatible scheduling, host-only expansion/orphan behavior, close/reopen, concurrent host/environment mutations, and forged identity rejection. Verification passed: `nix develop -c env DATABASE_URL=postgresql://mcamp@127.0.0.1:55484/postgres SQLX_OFFLINE=true cargo test --quiet --manifest-path packages/default/crates/cf-server/Cargo.toml --test poam_workflows -- --test-threads=1` (70 passed); `SQLX_OFFLINE=true nix develop -c cargo check --manifest-path packages/default/crates/cf-server/Cargo.toml --tests`; touched-file rustfmt check; `git diff --check`. Existing unrelated compiler warnings remain. Modified scope is only cf-server API models, POA&M handler/service/tests, and migration 0270; no SQLx metadata changed.

Focused frontend follow-up implemented without commit or push. `poam_api.rs` now mirrors the backend host/environment scope request and direct/effective detail contract. System Detail defaults each dialog open to host scope, reconstructs a complete scope-specific draft on every scope switch, keeps inherited environment state out of host fields, explains override removal/inheritance and environment prospective coverage, sends only the scope enum, and preserves existing typed assignee/POA&M reuse/focus/stale-response/viewer gates. Effective host and inherited environment row chips and tooltips are source-aware. Shared fixed-version copy now distinguishes exact versions, authoritative availability with version pending, and pending; blank versions no longer count as exact versions or block later exact-version merging. Fleet request serialization remains unchanged and has a regression asserting no scope field. Verification passed: focused component tests 16/16; focused fleet CVE tests 10/10; complete Web UI suite 440 passed/1 ignored; WASM cargo check; touched-file rustfmt check; `git diff --check`. Existing repository warnings remain. No frontend/backend contract mismatch was found. Preview remains waived/blocked by the other task's fixed-port stack, which was not touched.

Backend acceptance corrections completed without commit or push. Environment scheduled ownership and SQL/service coherence now exclude every exact host with an active host override. Host ACCEPTED retires only its inherited environment link; host OPEN restores that link only when the remaining inherited set is coherent. Compatible host-only POA&Ms can expand to environment ownership, incompatible schedules retain disjoint owners, and environment-origin POA&Ms return typed `poam_final_subject` instead of using the detached-host exception. Migration 0270 now limits that exception to one historical exact subject with no environment-disposition history. Verification passed: focused `system_cve_host` tests 3/3; full serial `poam_workflows` 70/70; `SQLX_OFFLINE=true cargo check --tests`; package `cargo fmt --check`; and `git diff --check`. Existing unrelated compiler warnings remain. Backend files are uncommitted; concurrent Web UI changes were not modified.

Focused host/environment triage follow-up committed and pushed to existing MR !329 as `0633e62f1838e7f944e5e6a517db89e0f39a2c93` (`TASK-326.2: Add host-scoped CVE triage`). Local and remote branch heads match, and the task worktree is clean. Final server review corrections include environment reopen ownership filtering and a typed conflict when a scheduled environment has no exact subject without a host override. Verification after those corrections passed: full `poam_workflows` 71/71 against a disposable PostgreSQL cluster on port 55484; Web UI tests 440 passed/1 ignored; JavaScript syntax and static browser contracts; touched-file rustfmt; and `git diff --check`. The earlier focused NixOS browser run passed `16-cves` but failed `12h-system-detail-cves-grouped-justification` because native trigger disabling dropped focus before dialog opener capture. Commit `0633e62f` fixes that path with guarded `aria-disabled`/`aria-busy` loading state. The user declined a post-fix VM rerun and directed commit/push, so real-browser verification of that focus fix remains outstanding. The task remains In Progress and MUST NOT return to Review until that required browser evidence is completed or explicitly waived as review-ready.

Final browser regression correction and review evidence: commit `7c4c27d35f7ce7c80a897d84797342cd232112e4` updates only `checks/web-ui/tests/integration-test.js`. The shared `assertTriageChip` helper now waits for refreshed exact inventory, re-expands the target package through `showInventory`, waits for the expected hydrated label, and retains exact label and title assertions. No production file changed; the `aria-disabled`/`aria-busy` focus-restoration fix remains intact. Cheap verification passed: `node --check checks/web-ui/tests/integration-test.js`; `CF_UI_STATIC_CONTRACTS=1 node checks/web-ui/tests/integration-test.js`; and `git diff --check`. The host runner does not classify 12h as a dev-stack workflow, so no host 12h run was available. The single authorized final VM command `CF_UI_TEST_STEPS="12h-system-detail-cves-grouped-justification,16-cves" nix build --impure "path:.#checks.x86_64-linux.web-ui" --no-link -L` passed both requested workflows (2/2) and captured 18 dark/light screenshots. Full command output: `/home/mcamp/.local/share/opencode/tool-output/tool_0bfa5906b0013CvesGaPBzf8e0`. Commit `7c4c27d3` was pushed to existing MR !329; local and remote SHA match, the task worktree is clean, and TASK-326.2 is ready for Review.

Review follow-up resumed at current branch head `7c4c27d35f7ce7c80a897d84797342cd232112e4` to fix filesystem-proven deploy-rs activatable NixOS wrapper equivalence and replace misleading user-facing Legacy CVE fallback wording with Historical wording. The task worktree is clean. Scope preserves exact-authority fail-closed lineage checks, historical scan immutability, existing triage focus restoration, and browser sequencing repair. Planned verification is focused agent/server/UI/unit/static coverage before any final browser decision.

Review follow-up implementation complete at pre-commit state. Agent and legacy server generation collectors now preserve direct canonical-root equality and recognize different activatable wrapper roots only when `init`, `system`, and `bin/switch-to-configuration` all canonicalize to the running toplevel's corresponding children. Filesystem tests cover direct generation 3079, production-shaped activatable generation 3079, unrelated systems, name-only false positives, missing profiles, and generation parsing. Exact inventory regression now proves Exact -> RetainedGenerationUnavailable -> CurrentStoreMismatch without changing query predicates. User-visible fallback copy now says Historical while wire/API `Legacy` identities remain unchanged. Browser fixture was corrected to include the current host/environment disposition DTO fields.

Verification passed: agent `system_state::tests` 7/7; server `models::system_states::tests` under `SQLX_OFFLINE=true`; isolated PostgreSQL `system_inventory_classifies_fallbacks_and_exact_precedence` 1/1; Web UI CVE tests 35/35; JavaScript syntax/static contracts; targeted rustfmt and `git diff --check`. Focused authoritative browser evidence: `16-cves` passed in `/home/mcamp/.local/share/opencode/tool-output/tool_0bfc6650c001WanmaPZ5e8AHmx`; after repairing its stale DTO fixture, `12ha-system-detail-cve-inventory-fallbacks` passed 1/1 with dark/light screenshots in `/nix/store/yx3hjrsnnbxajfdjbf7qbdmxin41228k-vm-test-run-crystal-forge-web-ui-mega-integration`. Host preview remains blocked/waived because TASK-440 owns the fixed ports.

Audit discovery requiring explicit scope decision: live migration-0259 exact-CVE view/functions and the compatibility justification handler filter invalid states before selecting the latest row. They can use an older valid state after a newer mismatch. Fixing this correctly requires a new additive migration replacing four database objects plus focused trigger/view regressions. No migration or security-boundary expansion was silently added to this wrapper/copy follow-up.

Committed and pushed review follow-up as `19352160` (`TASK-326.2: Recognize activatable NixOS generations`) to MR !329. Final task status remains In Progress pending the user's scope decision for the audited migration-0259 stale-state authority defect.
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
