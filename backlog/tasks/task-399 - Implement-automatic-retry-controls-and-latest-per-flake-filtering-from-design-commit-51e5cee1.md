---
id: TASK-399
title: >-
  Implement automatic retry controls and latest-per-flake filtering from design
  commit 51e5cee1
status: In Progress
assignee:
  - Matt Camp
created_date: '2026-07-24 03:26'
updated_date: '2026-07-24 03:35'
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
- [ ] #1 Admin > Server displays an "Automatic retries" card matching commit 51e5cee1 with build retry, evaluation retry, backoff, transient-only, Reset, and Save controls.
- [ ] #2 When no retry policy has been persisted, the UI and server report defaults of 2 build retries, 1 evaluation retry, 30-second backoff, and transient-only enabled.
- [ ] #3 The server rejects retry counts outside 0 through 5 and backoff values outside none, 10 seconds, 30 seconds, 1 minute, 2 minutes, and 5 minutes without partially updating the saved policy.
- [ ] #4 An authorized administrator can save the complete retry policy, receive visible success feedback, reload the application or restart the server, and observe the same values.
- [ ] #5 A non-administrator cannot read or mutate retry policy beyond existing authorization rules, and authorization failures are surfaced without changing persisted values.
- [ ] #6 Reset restores the last server-provided values and clears only unsaved edits; it does not persist a change or restore factory defaults.
- [ ] #7 A failed evaluation receives no more than the configured number of additional attempts and a failed build receives no more than the configured number of additional attempts; zero disables automatic retries for that job type.
- [ ] #8 Cancelled work and authorization failures are never automatically retried; transient-only mode excludes deterministic failures including invalid derivations and assertion failures, while disabling transient-only permits otherwise eligible failures.
- [ ] #9 Each retry preserves the original logical job inputs, records attempt lineage, applies the configured backoff asynchronously, and remains subject to existing session and job authorization checks.
- [ ] #10 Duplicate terminal/completion events or coordinator races cannot schedule more than one next attempt for the same failed attempt.
- [ ] #11 Saving new policy affects failures observed after the save succeeds without reopening historical terminal jobs or duplicating retries already scheduled under the prior policy.
- [ ] #12 Save validation, authorization, and server/persistence failures are visible and retain unsaved form edits so the administrator can correct or retry the operation.
- [ ] #13 Builds and Evaluations active and history/completed tabs expose a keyboard-accessible "Latest per flake" toggle with a programmatically exposed active/pressed state.
- [ ] #14 Each active/history tab identifies exactly one latest item per flake by greatest authoritative creation/enqueue timestamp with deterministic ID tie-breaking, independent of current table sort order.
- [ ] #15 Latest identity is computed over the complete tab result domain before pagination, search, status, and flake filters; existing filters and search combine correctly with latest-only filtering.
- [ ] #16 The latest build/evaluation for each flake displays the reference star and emphasized commit treatment while the toggle is off, and enabling the toggle hides every non-latest item.
- [ ] #17 The latest-only toggle remains enabled across active/history tab switches in the current view session while each tab computes latest items from its own data.
- [ ] #18 Latest markers and filtered rows recompute correctly after live updates, new jobs, cancellation, queue reorder, and pagination without stale or duplicate rows.
- [ ] #19 Existing multi-select, row selection, queue actions, details/log views, search, filters, live refresh, and infinite scrolling remain functional with latest-only disabled or enabled.
- [ ] #20 Builds and Evaluations show a filter-aware empty state when combined criteria yield no rows and allow users to clear the active filtering without misreporting that the underlying queue/history is empty.
- [ ] #21 Automated server tests cover retry policy defaults, validation, persistence, authorization, retry budgets, failure eligibility, cancellation exclusion, backoff, and duplicate-event idempotency.
- [ ] #22 Automated UI/state tests cover latest selection and tie-breaking, independent active/history scope, combined filters, pagination boundaries, live recomputation, retry form reset/save/error behavior, and accessibility state.
- [ ] #23 The authoritative web-ui check passes with behavioral assertions for all three affected views, and the MR includes screenshots of the Admin retry card plus Builds/Evaluations latest markers and enabled latest-only state.
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
LOCK: gpt-5.6-sol on reckless in /home/mcamp/code/crystal-forge/TASK-399-automatic-retries-latest-flake
<!-- SECTION:NOTES:END -->
