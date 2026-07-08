---
id: TASK-386
title: >-
  Visual drift audit: bring every view in line with the updated (2026-07-07)
  design example
status: Backlog
assignee: []
created_date: '2026-07-08 07:27'
labels:
  - design-parity
  - audit
  - web-ui
dependencies:
  - TASK-385
references:
  - packages/web-ui/src/views/
  - packages/web-ui/assets/app.css
  - checks/web-ui/design-parity/manifest.json
  - checks/web-ui/coverage-manifest.json
documentation:
  - >-
    backlog/docs/specs/doc-19 -
    Spec-All-views-visual-drift-audit-against-updated-design-example.md
  - docs/design/CrystalForge/
priority: high
ordinal: 329000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

The design example at `docs/design/CrystalForge/` was updated on 2026-07-07, but all prior parity work (the m-19/m-20 push: TASK-330/353/357/334/342.2/347.1/348.1, now closed) targeted an OLDER design snapshot. Nobody has systematically compared the implemented UI against the CURRENT design, so each view has unknown visual drift: changed labels, chips, spacing, stat strips, states, and possibly missing minor elements.

## Goal

Walk EVERY view against the updated design example, fix all SMALL (presentational-only) drift inline, and file follow-up Backlog tasks for every LARGE gap — producing (a) a design-identical UI for everything that doesn't need new data/flows, and (b) a complete, deduplicated list of follow-up tasks for everything that does.

**The complete audit procedure is doc-19** (`backlog/docs/specs/doc-19 - Spec-All-views-visual-drift-audit-against-updated-design-example.md`): the SMALL vs LARGE classification rule, the comparison method (design `serve.sh` + `run-ui-dev` / check screenshots + the design-parity harness), and the ordered 17-surface checklist with exact design-file → implementation-file mapping. Read it FIRST.

## The one rule (scope containment)

- **SMALL = fix here**: presentational-only, existing components, no new API data, no new routes, no new interaction flows, no backend changes.
- **LARGE = file a Backlog task, do NOT implement**: needs backend data/endpoints, new views/routes, new interaction flows — or is already owned by an open task (search first; see the do-not-duplicate list in doc-19 §3).
- When unsure → LARGE.

## Known facts

- Profile view is entirely MISSING (design `ProfileView.jsx`; former TASK-335 archived unmerged) → file a fresh follow-up, do not build it here.
- Open tasks that own specific gaps (do not duplicate): TASK-384 (deployment progress/rollback/recent activity), TASK-385 (flakes sync errors + sidebar badges), TASK-353.1, TASK-353.2, TASK-357.1, TASK-357.2, TASK-348.1.1.

## Non-Goals

Implementing anything LARGE; any backend change; refactors beyond what a presentational fix requires; mobile/responsive redesign; the surfaces owned by TASK-384/TASK-385 (audit them, but route findings to those tasks or new follow-ups).

## Architectural Constraints

- Only `packages/web-ui` view/component/CSS files change (plus check manifests/baselines).
- Follow existing component patterns; shared styles go in `assets/app.css`, not inline duplicates.
- Every intentional visual change that breaks a web-ui check assertion/baseline must update that assertion/baseline in the same MR.

## Deliverables

1. Inline fixes for all SMALL drift across the 17 audit surfaces (doc-19 §2 table).
2. Per-view audit record in task notes: "matches" / "fixed inline: …" / "follow-ups filed: TASK-…".
3. Follow-up Backlog tasks (Backlog Capture minimum: Problem + Desired Outcome + design file/line refs) for every LARGE gap.
4. Extended `checks/web-ui/design-parity/manifest.json` coverage for the views fixed.
5. MR with before/after screenshots per fixed view (GitLab uploads, never committed).

## Impact Areas

`packages/web-ui/src/views/*`, `packages/web-ui/src/components/*`, `packages/web-ui/assets/app.css`, `checks/web-ui/coverage-manifest.json`, `checks/web-ui/design-parity/manifest.json`.

## Risk

**Low-Medium.** Presentational-only changes; biggest risks are (a) scope creep — mitigated by the SMALL/LARGE rule and mandatory follow-up filing, and (b) breaking check baselines — mitigated by updating them in the same MR.

## Dependencies

- TASK-385 (flakes sync errors + sidebar badges) should merge first: it changes the sidebar and flakes surfaces this audit will inspect. TASK-384 merging first is also preferred for the same reason on Systems surfaces.

## Verification Plan (Tier 0/1)

Per doc-19 §4: `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test` in `packages/web-ui` (from `nix develop`); `nix build .#checks.x86_64-linux.web-ui --no-link` passing with updated baselines/assertions; `nix flake check` only if check definitions changed (state tier decision in MR). MR attaches before/after screenshots for every fixed view.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 All 17 audit surfaces from the doc-19 checklist have been compared against the updated design example using the doc-19 method, and the task notes contain a per-view audit record (matches / fixed inline / follow-ups filed)
- [ ] #2 Every SMALL (presentational-only) discrepancy found is fixed inline: labels, chips, colors, icons, spacing, typography, stat strips, filter bars, empty/loading/error states, modal/tray/drawer visuals - with no new API data, routes, flows, or backend changes introduced
- [ ] #3 Every LARGE gap has a filed follow-up Backlog task with Problem, Desired Outcome, and exact design file/line references; no LARGE gap was implemented in this task; no follow-up duplicates an open task from the doc-19 do-not-duplicate list
- [ ] #4 A follow-up task exists for the missing Profile view referencing components/ProfileView.jsx
- [ ] #5 checks/web-ui passes with baselines/assertions updated for every intentional visual change, and design-parity manifest coverage is extended for the views fixed inline
- [ ] #6 Only web-ui view/component/CSS files and check manifests/baselines were modified (no backend, no migrations)
- [ ] #7 fmt, clippy -D warnings, and cargo test pass in packages/web-ui; nix build .#checks.x86_64-linux.web-ui passes; MR attaches before/after screenshots for each fixed view via GitLab uploads (not committed)
<!-- AC:END -->
