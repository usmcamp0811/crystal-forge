---
id: TASK-399
title: >-
  Implement automatic retry controls and latest-per-flake filtering from design
  commit 51e5cee1
status: Review
assignee:
  - Matt Camp
created_date: '2026-07-24 03:26'
updated_date: '2026-07-25 02:23'
labels:
  - design-parity
  - web-ui
  - server
  - admin
  - retries
  - builds
  - evaluations
dependencies: []
references:
  - >-
    https://gitlab.com/crystal-forge/crystal-forge/-/commit/51e5cee17e3477686c70029275a88d4030178048
  - TASK-275
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/310'
documentation:
  - docs/design/CrystalForge/components/AdminView.jsx
  - docs/design/CrystalForge/components/BuildsView.jsx
  - docs/design/CrystalForge/components/EvalsView.jsx
  - docs/design/CrystalForge/components/Icon.jsx
  - docs/design/CrystalForge/components/Shell.jsx
  - docs/design/CrystalForge/data-admin.js
  - docs/design/CrystalForge/styles.css
priority: high
type: feature
ordinal: 398000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem
The production Crystal Forge application does not yet implement the two user-facing capabilities introduced by design commit `51e5cee17e3477686c70029275a88d4030178048`: administrators cannot configure automatic evaluation/build retries, and operators cannot quickly identify or isolate the newest evaluation/build for each flake.

## Goal
Translate the referenced prototype delta into production behavior across the server-owned domain model and the Dioxus web UI. Administrators must be able to persist retry policy safely, and Builds/Evaluations users must be able to recognize and filter to the latest item per flake without losing existing queue, history, search, selection, pagination, or live-refresh behavior.

The commit is the authoritative visual and interaction reference for this task. Its JSX fixtures and in-memory state are illustrative only: production data must come from existing authenticated APIs and server-owned persistence/job coordination rather than copied mock data or browser-only state.

## Required behavior
### Automatic retry policy
- Add an Admin > Server "Automatic retries" card matching the reference hierarchy and copy.
- Expose maximum build retries and maximum evaluation retries with allowed values 0 through 5, where 0 means no automatic retry.
- Expose backoff choices of none, 10 seconds, 30 seconds, 1 minute, 2 minutes, and 5 minutes.
- Expose an "Only retry transient failures" policy switch.
- When no persisted policy exists, use the reference defaults: 2 build retries, 1 evaluation retry, 30-second backoff, and transient-only enabled.
- "Save retry config" validates and persists the complete policy through an admin-authorized server API. The saved policy survives server restart and is shared by all web sessions.
- "Reset" discards unsaved edits and restores the last server-provided values; it does not silently overwrite persisted settings or restore factory defaults.
- A retry count is the number of additional attempts after the initial attempt. Failed attempts are retried only while their configured retry budget remains.
- Cancellation and authorization failures are never automatically retried. With transient-only enabled, deterministic evaluation/build failures such as invalid derivations and assertion failures are not retried. With transient-only disabled, otherwise retry-eligible failures may be retried regardless of transient classification.
- Retried work preserves the original logical job inputs and is linked to the original job/attempt so operators can distinguish attempts without creating unrelated duplicate work.
- Backoff is applied between failed completion and enqueue/start of the next attempt without blocking asynchronous job coordination.
- A saved policy governs failures observed after the save succeeds. Historical terminal jobs are not reopened, and changing policy does not duplicate an already scheduled retry.
- Save success, validation failure, authorization failure, and server/persistence failure are surfaced clearly; failed saves retain the user's edits for correction or retry.

### Latest per flake
- Add the star icon, latest-commit emphasis, and active-filter styling represented by the design commit using the production design system and theme tokens.
- Builds and Evaluations each provide a keyboard-accessible "Latest per flake" toggle on both active and history/completed tabs, with an exposed pressed/active state.
- For each tab independently, "latest" means the item with the greatest authoritative creation/enqueue timestamp for a flake; deterministic ID tie-breaking is used when timestamps are equal. Current table sort order must not change which item is latest.
- Latest determination is made across the complete result domain for the selected active/history tab before client pagination, search, status, or flake filters are applied. Search and existing filters then combine with the latest-only predicate.
- The latest item for every flake is visually marked with the star/emphasized commit treatment even when the latest-only toggle is off. Turning the toggle on shows only those marked items.
- The toggle state remains active when switching between active and history/completed tabs during the current view session, while each tab uses its own latest set.
- Live updates, cancellation, queue movement/reordering, multi-select, row actions, details/log surfaces, and infinite scrolling continue to work. Latest markers/results recompute when authoritative data changes without causing stale rows, duplicate rows, or incorrect selection.
- Empty states distinguish a genuinely empty queue/history from no results caused by combined search/filter/latest criteria and provide an appropriate way to clear active filtering.

## Non-goals
- Do not copy prototype fixtures or use browser-local state as the source of truth for retry policy or latest-item identity.
- Do not redesign unrelated Admin, Builds, or Evaluations surfaces.
- Do not change manual retry/re-evaluate semantics beyond what is required to add automatic retry attempts.
- Do not add retry counts, backoff values, or failure classes beyond those defined above without product approval.
- Do not fold in the broader Builds/Evaluations visual refactor tracked by TASK-275; coordinate overlapping files but keep this task to the commit delta.

## Architectural and safety constraints
- The server owns retry policy persistence, validation, transient-failure classification, retry-budget enforcement, and scheduling.
- Existing builder/agent session validation and server-issued job authorization remain enforced for every attempt.
- API-only builders must not access the Crystal Forge database directly.
- Retry scheduling must be idempotent under duplicate completion events and server/job-coordinator concurrency.
- Browser code must remain WASM-compatible, and nontrivial latest/filter state transitions should remain outside view markup where practical.
- Any persistence schema change requires a forward migration and matching SQLx offline metadata.

## Related work
TASK-275 touches the same Builds/Evaluations views but is not a dependency; implementation must coordinate to avoid duplicate refactoring or merge conflicts.

## Risk
High. This combines user-visible operational filtering with server-side retry behavior that can multiply work if retry budgets or idempotency are incorrect.

## Verification expectations
Use the repository Nix development environment. Add focused server/domain tests for policy validation, persistence, authorization, retry budgeting, transient classification, backoff scheduling, cancellation exclusion, and duplicate-event idempotency. Add deterministic UI/state tests for latest selection, tie-breaking, combined filters, tab scope, pagination boundaries, and live-data recomputation. Extend the authoritative `web-ui` check with behavioral assertions for editing/resetting/saving retry policy and toggling latest-per-flake in Builds and Evaluations. Capture MR screenshots of the Admin retry card and Builds/Evaluations latest markers and active filter in representative populated states. Run the relevant Rust/WASM checks and `nix build .#checks.x86_64-linux.web-ui`; run broader flake checks if implementation changes Nix, cross-package interfaces, or repository policy requires them.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Admin > Server displays an "Automatic retries" card matching commit 51e5cee1 with build retry, evaluation retry, backoff, transient-only, Reset, and Save controls.
- [x] #2 When no retry policy has been persisted, the UI and server report defaults of 2 build retries, 1 evaluation retry, 30-second backoff, and transient-only enabled.
- [x] #3 The server rejects retry counts outside 0 through 5 and backoff values outside none, 10 seconds, 30 seconds, 1 minute, 2 minutes, and 5 minutes without partially updating the saved policy.
- [x] #4 An authorized administrator can save the complete retry policy, receive visible success feedback, reload the application or restart the server, and observe the same values.
- [x] #5 A non-administrator cannot read or mutate retry policy beyond existing authorization rules, and authorization failures are surfaced without changing persisted values.
- [x] #6 Reset restores the last server-provided values and clears only unsaved edits; it does not persist a change or restore factory defaults.
- [x] #7 A failed evaluation receives no more than the configured number of additional attempts and a failed build receives no more than the configured number of additional attempts; zero disables automatic retries for that job type.
- [x] #8 Cancelled work and authorization failures are never automatically retried; transient-only mode excludes deterministic failures including invalid derivations and assertion failures, while disabling transient-only permits otherwise eligible failures.
- [x] #9 Each retry preserves the original logical job inputs, records attempt lineage, applies the configured backoff asynchronously, and remains subject to existing session and job authorization checks.
- [x] #10 Duplicate terminal/completion events or coordinator races cannot schedule more than one next attempt for the same failed attempt.
- [x] #11 Saving new policy affects failures observed after the save succeeds without reopening historical terminal jobs or duplicating retries already scheduled under the prior policy.
- [x] #12 Save validation, authorization, and server/persistence failures are visible and retain unsaved form edits so the administrator can correct or retry the operation.
- [x] #13 Builds and Evaluations active and history/completed tabs expose a keyboard-accessible "Latest per flake" toggle with a programmatically exposed active/pressed state.
- [x] #14 Each active/history tab identifies exactly one latest item per flake by greatest authoritative creation/enqueue timestamp with deterministic ID tie-breaking, independent of current table sort order.
- [x] #15 Latest identity is computed over the complete tab result domain before pagination, search, status, and flake filters; existing filters and search combine correctly with latest-only filtering.
- [x] #16 The latest build/evaluation for each flake displays the reference star and emphasized commit treatment while the toggle is off, and enabling the toggle hides every non-latest item.
- [x] #17 The latest-only toggle remains enabled across active/history tab switches in the current view session while each tab computes latest items from its own data.
- [x] #18 Latest markers and filtered rows recompute correctly after live updates, new jobs, cancellation, queue reorder, and pagination without stale or duplicate rows.
- [x] #19 Existing multi-select, row selection, queue actions, details/log views, search, filters, live refresh, and infinite scrolling remain functional with latest-only disabled or enabled.
- [x] #20 Builds and Evaluations show a filter-aware empty state when combined criteria yield no rows and allow users to clear the active filtering without misreporting that the underlying queue/history is empty.
- [x] #21 Automated server tests cover retry policy defaults, validation, persistence, authorization, retry budgets, failure eligibility, cancellation exclusion, backoff, and duplicate-event idempotency.
- [x] #22 Automated UI/state tests cover latest selection and tie-breaking, independent active/history scope, combined filters, pagination boundaries, live recomputation, retry form reset/save/error behavior, and accessibility state.
- [x] #23 The authoritative web-ui check passes with behavioral assertions for all three affected views, and the MR includes screenshots of the Admin retry card plus Builds/Evaluations latest markers and enabled latest-only state.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
## Implementation plan

1. **Persist and authorize retry policy**
   - Add a forward migration for a singleton retry-policy row with database constraints for counts `0..=5` and backoffs `0/10/30/60/120/300`, seeded to `2/1/30/transient-only`.
   - Add shared server domain validation/defaults, atomic query functions, and admin-only GET/PUT endpoints. Validate the complete payload before one upsert so invalid requests cannot partially update policy.
   - Add focused persistence, validation, and authorization tests.

2. **Make retries durable, linked, policy-driven, and idempotent**
   - Build retries: replace in-place requeue-on-failure with an atomic transaction that locks the source attempt, records its terminal failure, reads the policy effective at failure time, and inserts at most one linked child attempt with a durable `available_at` timestamp. Claims ignore future attempts. Preserve existing signed builder/session authorization for each claim and completion.
   - Evaluation retries: add durable evaluation-attempt lineage and a due timestamp; remove hard-coded retry counts/backoffs. Make claim/failure/cancellation transitions conditional and idempotent, and keep cancellation out of normal failure handling. Wake evaluation coordination for the next due attempt without blocking a worker task.
   - Centralize retry eligibility so cancellation, authorization/session failures, and derivation mismatch are always excluded; transient-only mode retries only explicitly transient classes. Extend the builder failure contract additively so deployed builders remain compatible.
   - Keep manual retry/re-evaluate behavior unchanged except for recording lineage needed to distinguish attempts.
   - Expose attempt number/parent/root and delayed retry timing through existing operational DTOs/details so operators can identify linked attempts.

3. **Compute latest-per-flake over complete server domains**
   - Add an immutable evaluation enqueue timestamp and stable flake identity to relevant DTOs.
   - Update active/history build and evaluation queries to rank one latest logical item per flake by authoritative enqueue/creation timestamp and deterministic ID tie-break before search/status/flake filters and pagination.
   - Return `is_latest_per_flake`, accept `latest_only`, and return enough unfiltered/filtered totals to distinguish genuinely empty domains from filter-empty results.
   - Preserve existing display sorting and growing-prefix infinite scroll while resetting pagination on every effective criterion.

4. **Implement production Admin, Builds, and Evaluations UI**
   - Add a testable retry-form state model with separate last-server and editable values. Reset restores only the last successful server value; failed saves retain edits; success/error/authorization states remain visible.
   - Add the Admin > Server Automatic retries card matching design commit `51e5cee1`.
   - Add the star icon/styles and one session-level keyboard-accessible `Latest per flake` pressed toggle to Builds and Evaluations. Render server-authoritative markers in both tabs, reconcile hidden selections, and prevent reorder operations on a latest-filtered subset.
   - Add filter-aware empty states and clear-filter actions without changing unrelated view layout.

5. **Verification and evidence**
   - Add DB/domain/handler tests for defaults, validation, persistence, authorization, budgets, eligibility, cancellation, backoff, policy timing, lineage, and duplicate-event concurrency.
   - Add pure web state tests for reset/save/error behavior, marker/filter composition, tab scope, pagination reset, live replacement, tie results supplied by the API, and selection reconciliation.
   - Extend the authoritative web-ui Playwright check with critical behavioral steps for Admin retry save/reset/error and latest toggles/markers in Builds and Evaluations; capture representative screenshots.
   - Run targeted Rust/WASM formatting and tests, SQLx preparation against a verified isolated repository database, server/web-ui Nix builds, `nix build .#checks.x86_64-linux.web-ui --no-link`, and broader flake checks because this changes schema and cross-package contracts.

## Material decision checkpoint

Before application-code writes, confirm the proposed compatibility/classification and historical-data choices: unknown failures from older builders are not retried when transient-only is enabled but may retry when disabled; manual retries start a fresh automatic budget as today; evaluation enqueue timestamps for existing rows are backfilled from the immutable Git commit timestamp with ID tie-breaking; and evaluation attempt lineage is persisted/exposed while existing commit-level log presentation remains unchanged unless implementation proves attempt-scoped logs are required for correctness.

Decision checkpoint approved: unclassified older-builder failures are not retried in transient-only mode; manual retries begin a fresh automatic budget; historical evaluation enqueue timestamps are backfilled from Git commit time with deterministic ID tie-breaking; evaluation attempt lineage is persisted/exposed while commit-level log presentation remains unchanged unless correctness requires otherwise.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
LOCK: gpt-5.6-sol on reckless in /home/mcamp/code/crystal-forge/TASK-399-automatic-retries-latest-flake

Research confirmed latest identity must be computed server-side before filters/pagination because clients only hold capped growing prefixes. Builds can use `build_jobs.created_at`; evaluations require a new immutable enqueue timestamp. Existing build/evaluation retry paths are hard-coded/in-place and require transactional policy-at-failure scheduling.

Implementation complete. All 23 acceptance criteria addressed; see final summary for evidence and residual risk notes.

MR !310 opened against dev: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/310

Branch pushed: TASK-399-automatic-retries-latest-flake (commit f9ddf15a).
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
## Summary

Implemented automatic build/evaluation retry policy and latest-per-flake filtering per design commit 51e5cee17e3477686c70029275a88d4030178048, across the server domain/persistence layer and the Dioxus web UI (Admin, Builds, Evaluations).

## Server
- Migration 0183: singleton admin-authorized `automatic_retry_policy` (build/eval retries 0-5, backoff none/10s/30s/1m/2m/5m, transient-only switch), defaults 2/1/30s/enabled, DB CHECK constraints, admin-only GET/PUT with complete-payload validation before one atomic upsert.
- Migration 0184: durable, immutable build/evaluation attempt lineage (parent/root ids, attempt_number, available_at backoff timestamp); unique index enforces at most one automatic child per source attempt; claims ignore future-dated attempts.
- Typed failure classification (transient/deterministic/authorization/cancelled/unknown) for build and evaluation failures; cancellation/authorization/derivation-mismatch never auto-retried; transient-only mode fails closed for unclassified legacy builder reports.
- Evaluation completion/failure/cancellation made transactional and attempt-scoped (stale/duplicate events cannot affect a newer attempt); restart recovery finalizes in-flight cancellations instead of reopening them.
- Due-time coordinator wakeup so configured backoff (incl. 0/10/30s) applies promptly without blocking job coordination.
- Migration 0185: immutable evaluation enqueue timestamp (backfilled from commit time); Builds/Evaluations active+history queries rank one latest item per stable flake by timestamp + id tie-break before search/status/flake filters and pagination; responses expose is_latest_per_flake, accept latest_only, and return domain_total/filtered_total.

## Web UI
- Admin > Server "Automatic retries" card matching the design commit, with testable reset/save/error form state (Reset restores only last server value with no persistence; failed save retains edits and shows an error; success is visible and persists across reload).
- Keyboard-accessible "Latest per flake" toggle (native button, aria-pressed) on Builds/Evaluations active+history tabs; one session-level toggle persists across tab switches while each tab computes its own latest domain server-side; server-authoritative latest markers render even when off; hidden-selection reconciliation; reorder disabled while latest-filtered; filter-aware empty states with a clear action.

## Verification
- cargo test (cf-protocol, cf-builder, cf-server, web-ui): all passing.
- cargo check --all-targets with SQLX_OFFLINE=true: passing (offline metadata regenerated against an isolated local PostgreSQL instance created specifically for this task, never a shared/default dev DB).
- nix build .#checks.x86_64-linux.web-ui: passing (exit 0). All 7 new TASK-399 steps are in the critical gating set and passed: 30a/30b/30c (Admin retries defaults/reset, save+reload, failed-save draft retention) and 15j/15k (Builds latest markers, combined filters/empty-state) and 26c/26d (Evaluations equivalents). Screenshots captured and attached to MR !310.
- A handful of pre-existing, non-critical/advisory web-ui steps also failed on this run (15h, 15i, 26, 26b, plus unrelated Compliance/Caches/Systems/Hardening steps). Root-caused: their underlying view code paths are byte-identical to dev (confirmed via git show dev:...), so they predate this change and are not regressions from TASK-399.
- Did not run full `nix flake check`; scope was server/web-ui packages and the web-ui check only, no flake/devshell/packaging changes, so targeted verification is proportional per repository policy.

## Discovered and filed separately
- TASK-400 (Backlog, not blocking): sqlx-cli's `migrate run` binary fails deterministically on a fresh database at pre-existing migration 0182, even though the same SQL applies cleanly via plain psql and via SQLx's library-level migrator (used by #[sqlx::test] and the server binary, both of which succeed). Does not affect this MR's own migrations (0183-0185), independently validated end-to-end via the library migrator in an isolated cluster.

## State
Pushed to branch TASK-399-automatic-retries-latest-flake (commit f9ddf15a). MR !310 opened against dev: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/310. Not merged; task moved to Review pending human/reviewer approval. Task worktree retained until merge per repository workflow.
<!-- SECTION:FINAL_SUMMARY:END -->
