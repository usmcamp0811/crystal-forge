---
id: TASK-283
title: Refactor Builds view UI/UX to match latest design mockup
status: In Progress
assignee:
  - '@ai-agent'
created_date: '2026-04-30 21:32'
updated_date: '2026-05-02 02:46'
labels:
  - ui
  - ux
  - builds
  - web-ui
  - design-system
milestone: UI/UX Design System
dependencies: []
references:
  - /home/mcamp/code/crystal-forge/crystal-forge/project/data-builds.js
priority: medium
ordinal: 4500
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem
The current Builds view does not fully match the latest UI/UX design direction and interaction model represented by the newest mockup artifacts.

## Goal
Refactor the Builds view implementation so layout, information hierarchy, visual styling, interaction patterns, and responsive behavior align with the latest Builds design mockup.

## Design Source
- Local mockup/data reference: `/home/mcamp/code/crystal-forge/crystal-forge/project/data-builds.js`
- Any companion local design assets in `/home/mcamp/code/crystal-forge/crystal-forge` that define the latest Builds view should be treated as authoritative for this task.

## Non-Goals
- No backend/API contract changes unless strictly required for UI parity.
- No unrelated redesign work outside Builds view surfaces.
- No broad refactors of shared component libraries unless directly required by Builds view requirements.

## Architectural Constraints
- Keep business logic out of views.
- Keep state/adapter logic separated from presentational components.
- Reuse existing UI patterns/components where possible; create new reusable components only when necessary for design parity.
- Preserve DTO compatibility unless explicit API change is approved in a separate task.

## Impact Areas
- `packages/web-ui/src/views/builds.rs`
- Builds-related components under `packages/web-ui/src/components/**`
- Builds adapter/state mapping files as needed
- UI test/checks in `checks/web-ui/tests/**` for Builds view assertions/screenshots

## Verification Plan
- Compile verification:
  - `nix develop -c cargo check` (web-ui crate)
- Targeted UI check verification:
  - `nix build .#checks.x86_64-linux.web-ui`
- Test evidence:
  - Update/add web-ui integration check steps that assert key Builds UI behavior from the new design.
  - Capture updated screenshot evidence from `web-ui` checks and attach to MR.
- Regression verification:
  - Ensure existing Builds interactions (queue/history display, status badges, action affordances) still function as expected.

## Risk Level
Medium: high visible surface area and potential interaction regressions if styling/layout and behavior updates are not aligned.

## Dependencies
- Latest design mockup artifacts in `/home/mcamp/code/crystal-forge/crystal-forge` must be accessible and unambiguous.
- No active conflicting UI redesign task modifying the same Builds view files during execution.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Builds view visual layout and styling match the latest local design mockup for desktop and mobile breakpoints.
- [ ] #2 Build queue, active jobs, history, worker/status sections, and key badges/controls reflect the mockup’s information hierarchy and wording.
- [ ] #3 Any new/changed Builds-specific components are reusable and keep presentation separated from state/data mapping.
- [ ] #4 No backend/API contract is changed as part of this task (unless separately approved and tracked).
- [ ] #5 `nix develop -c cargo check` (web-ui) succeeds after implementation.
- [ ] #6 `nix build .#checks.x86_64-linux.web-ui` succeeds and includes updated Builds-view screenshot/assertion coverage proving intended UI behavior.
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
## JSX Parity Checklist (must be identical to `project/components/BuildsView.jsx`)

### Interaction parity
- [ ] Queue row **Logs** action opens log modal immediately (same behavior as `onLog(b)`).
- [ ] Queue row **Cancel** icon visibility/behavior matches JSX treatment.
- [ ] `Queue build` button behavior/wiring matches JSX (remove extra mocked note behavior).
- [ ] Remove/adjust Dioxus-only confirm modal flow so parity behavior matches JSX reference.

### Queue table content parity
- [ ] First line in package column uses package field equivalent to `b.pkg` (not flake name).
- [ ] Second line uses derivation snippet equivalent to `b.drv.slice(0,40)…`.
- [ ] Third line shows `flake · commit` formatting matching JSX (no unintended shortening differences).
- [ ] Worker column fallback matches JSX (`—` when unavailable).
- [ ] Progress bar width is driven by real progress value (not fixed placeholder width).
- [ ] Status chip wording/casing/class presentation matches JSX (`b.meta`-style outcome).
- [ ] Queued time text format matches JSX `queuedAt` presentation.

### Detail panel parity
- [ ] Restore side-panel backdrop behavior matching JSX `side-panel-backdrop` semantics.
- [ ] Panel title uses package identity equivalent to `b.pkg`.
- [ ] Subheader line shows derivation/FQDN equivalent to `b.drv`.
- [ ] Panel section/class structure matches JSX panel hierarchy (`panel-head`, `panel-body`, `panel-section`, `kv-grid`, `panel-actions`) or renders identically.
- [ ] Detail metadata values map 1:1 with JSX fields (`flake`, `commit`, `worker`, `arch`, `queuedAt`, `dur`, `attempts`, `logLines`).
- [ ] Cancel action in panel matches JSX behavior (no unintended close side effects if not present in JSX).

### Log modal parity
- [ ] Modal header includes `Build log — <pkg>` and derivation subtitle equivalent to JSX.
- [ ] Log stream renders structured timestamp/level/message lines (not raw blob only).
- [ ] Include modal footer actions equivalent to JSX (`Download`, `Close`).
- [ ] Include live-stream caret affordance equivalent to JSX (`▍`) if present.
- [ ] Modal sizing/width behavior matches JSX (`min(800px,98vw)` equivalent).
- [ ] Auto-scroll-to-bottom behavior matches JSX mount behavior.

### Worker card parity
- [ ] Worker card spacing/padding/gap match JSX card rhythm.
- [ ] Host line typography matches JSX (font size/mono/muted).
- [ ] Slot bar height/background/radius match JSX metrics.
- [ ] Status chip presentation matches JSX chip treatment.
- [ ] Worker status support/visual fallback matches JSX expectations (including offline handling where relevant).

### Metrics strip parity
- [ ] Metrics strip uses JSX-equivalent card/accent/label/value composition.
- [ ] Metric card spacing/height/typography match JSX stat-strip proportions.
- [ ] Accent bar placement/size/color handling matches JSX stat-accent behavior.

### Tabs and shell parity
- [ ] Tabs match `sd-tabs`/`sd-tab active` visual behavior exactly.
- [ ] Queue card shell spacing/borders/overflow match JSX card structure.
- [ ] Table styling matches JSX `sys-table` behavior (header, row, selected state, action alignment).

### Data wording and simplification parity
- [ ] Reduce non-reference complexity where it alters parity (e.g., completed filter/sort controls/logic not represented in JSX output behavior).
- [ ] Remove or isolate unused alternate queue-table implementation paths not part of JSX parity target.

### Pixel-level polish parity
- [ ] Iconography style is equivalent to JSX icon treatment (refresh/log/cancel).
- [ ] Border radius, shadows, and micro-spacing are visually indistinguishable from JSX.
- [ ] Typography line-height and size across headers/rows/panel are visually indistinguishable from JSX.

### Final parity proof requirements
- [ ] Provide side-by-side screenshot evidence for each major section: page head, stat strip, workers, queue tabs/table, detail panel, log modal.
- [ ] Ensure `nix build .#checks.x86_64-linux.web-ui` captures/validates the updated UI behavior and screenshots.
<!-- SECTION:NOTES:END -->
