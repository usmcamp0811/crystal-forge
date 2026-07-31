---
id: TASK-393
title: >-
  Implement the 2026-07-14 Builders design delta for side-panel and open-flow
  parity
status: Review
assignee: []
created_date: '2026-07-14 00:00'
updated_date: '2026-07-14 00:00'
labels:
  - design-parity
  - builders
  - web-ui
  - sprint-ready
dependencies:
  - TASK-392
references:
  - commit 2cc51a2d (`design changes builders`)
  - docs/design/CrystalForge/components/BuildersView.jsx
  - packages/web-ui/src/views/builders.rs
  - packages/web-ui/src/components/builders/
  - checks/web-ui/
documentation:
  - backlog/docs/specs/doc-19 - Spec-All-views-visual-drift-audit-against-updated-design-example.md
priority: high
ordinal: 393000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem Statement

The design example changed again in commit `2cc51a2d` (`design changes builders`)
on 2026-07-14. This update adds a new Builders interaction pattern that the
current shipped UI does not yet have: cards and table rows open a builder detail
side panel, the panel surfaces status/registration/slot-use/details/environment
information, and edit/register actions now hand off from that panel instead of
being the only primary row/card interaction.

The original Builders parity task (`TASK-364`) is already Done against the prior
reference state, and old backlog items such as `TASK-346.1` do not capture this
new delta precisely. Without a new scoped task, the Builders surface will drift
again from the latest design source and parity work will lack an objective,
current acceptance contract.

## Goal

Bring the Builders surface into parity with the latest 2026-07-14 design delta
only for the newly changed interaction and layout behavior: click-to-open cards
and rows, the builder detail side panel, the updated open/edit/register flow,
and any required screenshot/assertion coverage updates.

## Authoritative Commit Delta

- Commit: `2cc51a2d` (`design changes builders`)
- Implement from the exact design-file changes in that commit:
  - `docs/design/CrystalForge/components/BuildersView.jsx`
- Treat the commit diff as the authoritative source for what changed in scope:
  click-to-open cards/rows, selected builder state, `BuilderPanel`, and the
  updated edit/register handoff flow.

## Non-Goals

- Re-doing the entire Builders parity effort from scratch.
- Backend/schema/API changes unless a tiny UI contract gap is discovered and is
  strictly required.
- Broad builder runtime/scheduling/auth fixes.
- Unrelated Builders modal redesign work that is not part of commit `2cc51a2d`.
- Changes to non-Builders surfaces except shared web-ui primitives strictly
  required for this panel pattern.

## Scope Notes

This task is driven specifically by the `BuildersView.jsx` delta in commit
`2cc51a2d`, which adds:

- local selected/view state for a builder detail panel
- click-to-open behavior for cards and table rows
- a `BuilderPanel` side panel with panel head/body/actions
- panel sections for status, registration warning/fingerprint, slot use, load,
  details, and environments
- updated row/card action semantics where edit/register remains available but is
  no longer the only interaction path

## Architectural Constraints

- Follow the existing `packages/web-ui` Builders view/component split.
- No business logic in presentation components beyond view-local formatting.
- Reuse existing side-panel patterns/styles where practical; do not introduce a
  one-off interaction model for Builders if an existing pattern already exists.
- Keep scope limited to Builders and directly required shared styling/test files.
- Any new check baselines or assets must be tracked in Git.

## Verification Plan

Automated:

- nix develop -c cargo fmt --manifest-path packages/web-ui/Cargo.toml --all -- --check
- nix develop -c cargo clippy --manifest-path packages/web-ui/Cargo.toml --all-targets -- -D warnings
- nix develop -c cargo test --manifest-path packages/web-ui/Cargo.toml
- nix build .#checks.x86_64-linux.web-ui --no-link

Manual:

- Compare Builders cards mode, table mode, and the new detail side panel against
  the updated `docs/design/CrystalForge/components/BuildersView.jsx` reference
  at desktop width.
- Verify card click and row click both open the intended builder panel.
- Verify panel action button text/behavior matches the design for both normal
  builders and unregistered builders (`Edit builder` vs `Register`).
- Capture MR screenshots for the changed Builders states using deterministic
  local/web-ui-check output.

## Impact Areas

- `packages/web-ui/src/views/builders.rs`
- `packages/web-ui/src/components/builders/`
- `packages/web-ui/assets/app.css` (only if shared panel styling is needed)
- `checks/web-ui/`

## Risk Level

Low-Medium.

The change is UI-focused and scoped to one surface, but it affects both cards and
table interactions plus screenshot coverage. The main risk is regressing existing
edit/register behavior while adding the new panel flow.

## Dependencies

- `TASK-392` must land first, per maintainer direction, because this Builders
  delta follows the same 2026-07-14 design-update wave and should stack after
  those shared parity changes.

## Follow-Up Guidance

If implementation uncovers additional Builders drift outside the `2cc51a2d`
delta, file a separate Backlog task instead of expanding this one.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria

<!-- AC:BEGIN -->
- [ ] #1 The shipped Builders UI matches the 2026-07-14 design delta from commit `2cc51a2d` for the changed interaction pattern: cards and table rows open a builder detail side panel, and the panel layout/content matches the reference within Dioxus constraints
- [ ] #2 The builder detail side panel includes the design-matched sections for status, registration warning/fingerprint when applicable, slot use/load, details, environments, and panel actions
- [ ] #3 Existing edit/register flows still work, but now hand off correctly from the new open-flow semantics: clicking the row/card opens the panel, while edit/register actions remain available and correctly labeled
- [ ] #4 Both registered and unregistered builder states are covered in the implementation and screenshot/assertion coverage, including the unregistered warning/banner + fingerprint treatment
- [ ] #5 Only Builders files and directly required shared styling/check files are modified; unrelated Builder backend/runtime behavior stays out of scope
- [ ] #6 `cargo fmt`, `cargo clippy -- -D warnings`, `cargo test`, and `nix build .#checks.x86_64-linux.web-ui --no-link` pass from the repository dev environment
- [ ] #7 Any newly discovered Builders parity work outside this delta is filed as a separate Backlog task rather than implemented here
<!-- AC:END -->

## Task Notes

MR: !300 (https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/300)

### Data Model Gaps (Backend Contract)

The BuilderPanel implementation matches the design structure exactly, but displays
placeholder values for metrics not currently available in BuilderSummary:

**Load percentage** (panel "Slot use" section, second bar):
- Design: `w.load` (0.0-1.0 fraction, shown as percentage)
- Current: Always displays "—" and 0% bar
- Reason: BuilderSummary has no load metric field
- Impact: Panel shows slot utilization correctly, but system load is unavailable

**Builds completed in 24h** (panel "Details" section, "Built 24h" row):
- Design: `w.completed24h` and `w.failed24h` counts
- Current: Always displays "—"
- Reason: BuilderSummary does not include build completion metrics
- Impact: Users cannot see recent build throughput in the panel

These are **not UI bugs**. The panel correctly renders all fields available in the
current API contract. Adding these metrics would require:

1. Load metric:
   - Builders report load average in heartbeat
   - Server stores/exposes in BuilderSummary
   - Query: `b.last_reported_load_avg as load_pct`

2. Build metrics:
   - Aggregate build_jobs WHERE builder_id = b.id AND completed_at > now() - interval '24 hours'
   - Add `completed_24h` and `failed_24h` to BuilderSummary
   - Query: Complex aggregation with GROUP BY and time filter

If these metrics become available, update:
- `packages/default/src/models/builders.rs` (BuilderSummary struct)
- `packages/default/src/queries/builders.rs` (list_builders query)
- `packages/web-ui/src/api/models.rs` (BuilderSummary struct)
- `packages/web-ui/src/components/builders/builder_panel.rs` (remove placeholder logic)

### Environment Ordering

Fixed: The SQL query now uses `ORDER BY e.name` in the json_agg() to ensure
deterministic environment pill ordering in the panel.
