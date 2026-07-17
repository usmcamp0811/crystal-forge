---
id: TASK-392
title: >-
  Implement the 2026-07-14 design delta for Flakes, Environments, Caches,
  System Detail, and Shell
status: Review
assignee: []
created_date: '2026-07-14 00:00'
updated_date: '2026-07-14 00:00'
labels:
  - design-parity
  - web-ui
  - sprint-ready
dependencies:
  - TASK-384
  - TASK-385
references:
  - commit 65a43af1 (`update design`)
  - docs/design/CrystalForge/app.jsx
  - docs/design/CrystalForge/components/CachesView.jsx
  - docs/design/CrystalForge/components/EnvironmentsView.jsx
  - docs/design/CrystalForge/components/FlakesView.jsx
  - docs/design/CrystalForge/components/Shell.jsx
  - docs/design/CrystalForge/components/SystemDetail.jsx
  - docs/design/CrystalForge/styles.css
  - packages/web-ui/src/views/caches.rs
  - packages/web-ui/src/views/environments_list.rs
  - packages/web-ui/src/views/flakes_list.rs
  - packages/web-ui/src/views/system_detail.rs
  - packages/web-ui/src/components/layout/
  - checks/web-ui/
priority: high
ordinal: 392000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem Statement

The design example was updated again in commit `65a43af1` (`update design`) on
2026-07-14, after the earlier 2026-07-07 parity/audit planning work. This new
delta is narrower than TASK-386's full-fleet audit, but it is concrete and
implementation-relevant: it changes the expected behavior and layout for the
Shell navigation ordering, Flakes deep-link tray behavior, Environments side
panel flow, Caches cards/table + side panel flow, and System Detail commit/build
cross-links plus commit peek behavior.

Without a dedicated task, these newly introduced reference changes are easy to
miss or get mixed into the broader audit backlog. That creates two risks:

1. reviewers compare the shipped UI against an out-of-date local parity target;
2. future parity work duplicates or partially implements the new delta without a
   single scoped acceptance contract.

## Goal

Implement the UI changes introduced by the 2026-07-14 design update exactly for
the touched surfaces only: Flakes, Environments, Caches, System Detail, shared
Shell navigation order, and any required shared web-ui styling/hooks needed to
support those flows. The shipped web UI should match the updated reference for
these specific surfaces, while preserving existing real-data behavior and adding
or updating screenshot/assertion coverage for the changed states.

## Authoritative Commit Delta

- Commit: `65a43af1` (`update design`)
- Implement from the exact design-file changes in that commit:
  - `docs/design/CrystalForge/app.jsx`
  - `docs/design/CrystalForge/components/CachesView.jsx`
  - `docs/design/CrystalForge/components/EnvironmentsView.jsx`
  - `docs/design/CrystalForge/components/FlakesView.jsx`
  - `docs/design/CrystalForge/components/Shell.jsx`
  - `docs/design/CrystalForge/components/SystemDetail.jsx`
  - `docs/design/CrystalForge/styles.css`
- Any work outside those design-file deltas is out of scope unless it is strictly
  required to make the changed references function in the shipped web UI.

## Non-Goals

- Full all-view drift audit across every surface (that remains TASK-386 scope).
- Backend/API/schema changes unless a tiny contract adjustment is strictly
  required to expose already-available UI data.
- New product surfaces not touched by commit `65a43af1`.
- Reworking unrelated visual debt near the changed views.
- Replacing existing real behavior with new mock-only behavior.

## Acceptance Scope Notes

The task is driven by the exact design-reference delta in commit `65a43af1`:

- `components/CachesView.jsx`: card/table toggle, cache cards, cache detail side
  panel, cache focus/deep-link behavior, system cross-links.
- `components/EnvironmentsView.jsx`: environment detail side panel, click-to-open
  cards/rows, cache/system/compliance cross-links.
- `components/SystemDetail.jsx`: commit peek via shared Flake tray, clickable
  commit/build affordances in Overview/Deploy/History-adjacent flows.
- `components/FlakesView.jsx`: tray accepts `focusMeta` and supports synthetic
  focused commits with real caller-provided metadata.
- `components/Shell.jsx`: Operations nav ordering change (Evaluations before
  Builds).
- `app.jsx` and `styles.css`: wiring and shared styles required by the above.

## Architectural Constraints

- Follow existing `packages/web-ui` view/component boundaries; no business logic
  in presentational components.
- Prefer reusing existing side-panel/tray patterns rather than introducing a new
  parallel interaction model.
- Keep backend changes out unless the UI cannot represent the updated design with
  already-available data.
- If a new reusable helper/component is introduced, it must be shared by at
  least two changed surfaces or clearly reduce duplication.
- Any new screenshot baselines or check assets must be tracked in Git.

## Verification Plan

Automated:

- nix develop -c cargo fmt --manifest-path packages/web-ui/Cargo.toml --all -- --check
- nix develop -c cargo clippy --manifest-path packages/web-ui/Cargo.toml --all-targets -- -D warnings
- nix develop -c cargo test --manifest-path packages/web-ui/Cargo.toml
- nix build .#checks.x86_64-linux.web-ui --no-link

Manual:

- Compare the implemented Flakes, Environments, Caches, System Detail, and Shell
  surfaces against the updated `docs/design/CrystalForge/` reference rendered at
  desktop width.
- Capture MR screenshots for each changed surface using the web-ui check output
  or another deterministic local run aligned with the updated design states.
- Verify commit/cross-link flows: System Detail commit link opens the Flake tray
  with the intended commit selected, Environments panel links route to Caches /
  Systems / Compliance as designed, and Caches panel links route to Systems.

## Impact Areas

- UI
- Shared styling
- Screenshot/assertion coverage

Primary files are expected under:

- `packages/web-ui/src/views/`
- `packages/web-ui/src/components/`
- `packages/web-ui/assets/app.css`
- `checks/web-ui/`

## Risk Level

Medium.

This is primarily UI work, but it spans multiple linked surfaces and shared
interaction patterns. The main risks are parity drift between related views,
regression in navigation/deep-link behavior, and screenshot/check churn.

## Dependencies

- TASK-384 should land first because this delta builds on current System Detail
  behavior and adjacent systems interactions.
- TASK-385 should land first because it also touches Flakes/Shell behavior and
  nearby parity assertions.

## Follow-Up Guidance

If implementation reveals additional drift outside the files touched by
`65a43af1`, file a separate Backlog task (or route it to TASK-386 if that task
is later selected) instead of expanding this task.
<!-- SECTION:DESCRIPTION:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
LOCK: opencode-agent on host in /home/mcamp/code/crystal-forge/TASK-392-design-delta-2026-07-14

Branch: TASK-392-design-delta-2026-07-14
Base: dev (6efa7bd1 — after TASK-385 merged)
MR: !301 (https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/301)

### DB Coordination with TASK-385
TASK-385 (in Review, MR !298) introduces migrations 0158 (flake_sync_status) and 0159 (user_alert_acknowledgments).
TASK-392 is pure UI work — no new DB migrations required.
Branch from dev; if TASK-385 merges first, rebase onto the updated dev before opening MR.
<!-- SECTION:NOTES:END -->

## Acceptance Criteria

<!-- AC:BEGIN -->
- [ ] #1 The shipped web UI matches the 2026-07-14 design-reference delta for the touched surfaces only: Flakes tray commit focus behavior, Environments detail panel flow, Caches cards/table/panel flow, System Detail commit/build cross-links and commit peek flow, and Shell Operations nav order
- [ ] #2 Environments rows/cards open a detail side panel matching the updated design, and from that panel users can navigate to the referenced cache, systems, and compliance bundle targets using existing app navigation patterns
- [ ] #3 Caches supports both cards and table modes per the updated design, exposes the cache detail side panel, and supports focus/open behavior from cross-links without breaking existing edit flows
- [ ] #4 System Detail commit-related affordances open the shared Flakes tray focused on the intended commit, including deployed or generated commits that are not already present in the tracked commit list, using the updated reference behavior for caller-provided metadata
- [ ] #5 Any required shared styles and app wiring introduced by the design delta are implemented in the existing shared UI layers without introducing unrelated layout or behavior regressions
- [ ] #6 checks/web-ui coverage and screenshots are updated for every intentionally changed state, including at minimum the changed Flakes, Environments, Caches, and System Detail surfaces
- [ ] #7 `cargo fmt`, `cargo clippy -- -D warnings`, `cargo test`, and `nix build .#checks.x86_64-linux.web-ui --no-link` pass from the repository dev environment
- [ ] #8 No unrelated surfaces are brought into scope, and any newly discovered out-of-scope parity gaps are filed as separate Backlog tasks instead of being implemented here
<!-- AC:END -->
